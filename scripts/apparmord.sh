# ~/.GH/pass-secret-service/scripts/apparmord.sh
# ----------------------------------------------
# Copyright (C) 2025 Qompass AI, All rights reserved

sudo apparmor_parser -r /etc/apparmor.d/local.qompass-secret-service
sudo aa-enforce local.qompass-secret-service
