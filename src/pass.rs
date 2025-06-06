use std::{
    collections::HashMap,
    env,
    fs::{FileType, Metadata},
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{
    fs::{
        DirBuilder, File, OpenOptions, metadata, read, read_dir, read_to_string, remove_dir_all,
        remove_file,
    },
    io::AsyncWriteExt,
    process::Command,
};

use crate::error::{Error, Result};

#[derive(Debug)]
pub struct PasswordStore {
    pub directory: PathBuf,
    gpg_opts: Option<String>,
    file_mode: u32,
    dir_mode: u32,
}

impl PasswordStore {
    /// Initialize this PasswordStore instance from env vars
    pub fn from_env() -> Result<Self> {
        let mut env: HashMap<String, String> = env::vars().collect();

        // Either ~/.password-store or $PASSWORD_STORE_DIR
        let directory = env
            .get("PASSWORD_STORE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = Path::new(env.get("HOME").expect("$HOME must be set"));
                home.join(".password-store")
            });

        let gpg_opts = env.remove("PASSWORD_STORE_GPG_OPTS");

        let umask = env
            .get("PASSWORD_STORE_UMASK")
            .and_then(|s| u32::from_str_radix(s, 8).ok())
            .unwrap_or(0o077);

        // lower 3 octal digits
        let dir_mode = !umask & 0o777;
        // lower 3 digits without execute bit
        let file_mode = !(umask | 0o111) & 0o777;

        Ok(Self {
            directory,
            gpg_opts,
            dir_mode,
            file_mode,
        })
    }

    fn get_full_secret_path(&self, path: impl AsRef<Path>) -> PathBuf {
        let mut path = self.directory.join(path);

        // add .gpg to the end if necessary
        if !path.ends_with(".gpg") {
            let os_str = path.as_mut_os_string();
            os_str.push(".gpg");
        };

        path
    }

    fn make_gpg_process(&self) -> Command {
        let mut command = Command::new("gpg");

        // apply the gpg opts
        if let Some(opts) = &self.gpg_opts {
            command.args(opts.split_ascii_whitespace());
        }

        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        command
    }

    /// Read a single password at the given path
    pub async fn read_password(&self, path: impl AsRef<Path>, can_prompt: bool) -> Result<Vec<u8>> {
        let contents = read(self.get_full_secret_path(path)).await?;

        let mut command = self.make_gpg_process();

        if !can_prompt {
            // don't activate pinentry if we can't prompt
            command.arg("--pinentry-mode=error");
        }

        command.arg("--decrypt").arg("-");

        let mut process = command.spawn()?;

        let mut stdin = process.stdin.take().expect("child has stdin");

        tokio::task::spawn(async move { stdin.write_all(&contents).await });

        let output = process.wait_with_output().await?;
        if output.status.success() {
            // gpg decrypted successfully
            Ok(output.stdout)
        } else {
            Err(Error::GpgError(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }

    async fn get_gpg_id(&self, dir: impl AsRef<Path>) -> Result<String> {
        for component in dir.as_ref().ancestors() {
            let gpg_id_path = component.join(".gpg-id");
            match read_to_string(gpg_id_path).await {
                Ok(value) => return Ok(value.trim().into()),
                // not found, continue
                Err(ref e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => Err(e)?,
            }

            // at the root pass dir
            if component == self.directory {
                break;
            }
        }
        // we couldn't find a gpg key
        return Err(Error::NotInitialized);
    }

    async fn ensure_dirs(&self, dir: impl AsRef<Path>) -> Result {
        // create this dir
        Ok(DirBuilder::new()
            .recursive(true)
            .mode(self.dir_mode)
            .create(dir)
            .await?)
    }

    /// write a single password
    pub async fn write_password(&self, path: impl AsRef<Path>, value: Vec<u8>) -> Result {
        let full_path = self.get_full_secret_path(path);

        let dir = full_path.parent().expect("path is a file");

        self.ensure_dirs(dir).await?;

        let gpg_id = self.get_gpg_id(dir).await?;

        let mut process = self
            .make_gpg_process()
            .arg("--recipient")
            .arg(gpg_id)
            .arg("--encrypt")
            .arg("-")
            .spawn()?;

        let mut stdin = process.stdin.take().expect("child has stdin");

        tokio::task::spawn(async move { stdin.write_all(&value).await });

        let output = process.wait_with_output().await?;
        if output.status.success() {
            // encryption successful

            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .mode(self.file_mode)
                .open(full_path)
                .await?;

            file.write_all(&output.stdout).await?;

            Ok(())
        } else {
            Err(Error::GpgError(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }

    pub async fn delete_password(&self, path: impl AsRef<Path>) -> Result {
        let full_path = self.get_full_secret_path(path);
        match remove_file(full_path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /****** Some useful FS utilities ******/

    /// list the file and directories inside a parent directory
    pub async fn list_items(&self, dir: impl AsRef<Path>) -> Result<Vec<(FileType, String)>> {
        let dir = self.directory.join(dir);
        self.ensure_dirs(&dir).await?;

        let mut dir_items = read_dir(dir).await?;

        let mut items = vec![];

        while let Some(item) = dir_items.next_entry().await? {
            let file_type = item.file_type().await?;
            let name = item.file_name().to_string_lossy().into_owned();
            items.push((file_type, name));
        }

        Ok(items)
    }

    /// open a file for writing
    pub async fn open_file(&self, file_path: impl AsRef<Path>) -> Result<File> {
        let path = self.directory.join(file_path);
        self.ensure_dirs(path.parent().expect("path is not a file"))
            .await?;

        Ok(OpenOptions::new()
            .write(true)
            .create(true)
            .read(true)
            .mode(self.file_mode)
            .open(path)
            .await?)
    }

    /// get metadata on a file
    pub async fn stat_file(&self, file_path: impl AsRef<Path>) -> Result<Metadata> {
        let path = self.directory.join(file_path);
        self.ensure_dirs(path.parent().expect("path is not a file"))
            .await?;

        Ok(metadata(path).await?)
    }

    /// make a dir and all its parents
    pub async fn make_dir(&self, dir: impl AsRef<Path>) -> Result {
        self.ensure_dirs(self.directory.join(dir)).await
    }

    /// recursively remove a dir
    pub async fn remove_dir(&self, dir: impl AsRef<Path>) -> Result {
        Ok(remove_dir_all(self.directory.join(dir)).await?)
    }
}
