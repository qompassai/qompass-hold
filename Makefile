# /qompassai/qompass-hold/Makefile
# Qompass AI Qompass-Hold Makefile
# Copyright (C) 2025 Qompass AI, All rights reserved
####################################################
APPARMOR_PROFILE=local.qompass-hold
APPARMOR_PATH=/etc/apparmor.d/$(APPARMOR_PROFILE)
SERVICE_NAME=qompass-hold.service
USER_UNIT_DIR=$(HOME)/.config/systemd/user
.PHONY: harden install-apparmor reload-apparmor reload-systemd
harden: install-apparmor reload-apparmor reload-systemd
install-apparmor:
	sudo install -Dm644 apparmor/$(APPARMOR_PROFILE) $(APPARMOR_PATH)
reload-apparmor:
	sudo apparmor_parser -r $(APPARMOR_PATH)
	sudo aa-enforce $(APPARMOR_PROFILE)
reload-systemd:
	systemctl --user daemon-reload
	systemctl --user restart $(SERVICE_NAME)
