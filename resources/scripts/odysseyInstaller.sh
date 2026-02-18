#!/usr/bin/env bash

SCRIPT_NAME=$(basename "$0")
readonly SCRIPT_NAME

VALID_TARGETS=("armv7-unknown-linux-gnueabihf" "aarch64-unknown-linux-gnu" "x86_64-unknown-linux-gnu")

DEF_RELEASE="latest"
DEF_TARGET=${VALID_TARGETS[0]}
DEF_DIR="/opt/odyssey"
DEF_CONFIG="default.yaml"

usage() {
cat <<EOF
Usage: $SCRIPT_NAME [-r RELEASE] [-t TARGET] [-d DIR] [-s] [-h]

This script downloads and unpacks the specified release of Odyssey,
for the specified system architecture target.

Options:
    -r RELEASE      The release version to be downloaded.
                    Defaults to $DEF_RELEASE

    -t TARGET       The system architecture target.
                    Must be one of the following values: $(printf '\n';printf '\t\t\t\t\t%s\n' "${VALID_TARGETS[@]}")
                    Defaults to $DEF_TARGET

    -d DIR          The directory in which to install the Odyssey executable.
                    Defaults to $DEF_DIR
    
    -c CONFIG_FILE  The name of the configuration file to be used.
                    Must be one of the configuration templates included in the odyssey release's configs directory.
                    Defaults to $DEF_CONFIG

    -cd CONFIG_DEST The desired destination for the installed config file.
                    Defaults to $DEF_DIR/$DEF_CONFIG

    -s              Create a systemd service for Odyssey

    -h              Print this message

EOF
}

RELEASE=$DEF_RELEASE
TARGET=$DEF_TARGET
DIR=$DEF_DIR
CONFIG=$DEF_CONFIG
CONFIG_DEST="$DEF_DIR/odyssey.yaml"

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -r | --release )
                RELEASE=$2
                shift
                shift
                ;;
            -t | --target)
                TARGET=$2
                shift
                shift
                ;;
            -d | --dir)
                DIR=$2
                shift
                shift
                ;;
            -c | --config)
                CONFIG_DIR=$2
                shift
                shift
                ;;
            -cd | --config-dest)
                CONFIG_DEST=$2
                shift
                shift
                ;;
            -s)
                CREATE_SERVICE="true"
                shift
                ;;
            -h | --help)
                usage
                exit 0
                ;;
            *)
                echo "Unknown option $1"
                usage
                exit 1
                ;;
        esac
    done  
}

download_odyssey() {
    if [ "$RELEASE" == "latest" ]; then
        odyssey_url="https://github.com/TheContrappostoShop/Odyssey/releases/latest/download/odyssey_$TARGET.tar.gz"
    else
        odyssey_url="https://github.com/TheContrappostoShop/Odyssey/releases/download/$RELEASE/odyssey_$TARGET.tar.gz"
    fi

    mkdir -p "${DIR}"
    mkdir -p `dirname "${CONFIG_DEST}"`

    wget "${odyssey_url}" -O - | tar -C "${DIR}" -xz

    cp "${DIR}/configs/${CONFIG}" "${CONFIG_DEST}"
}

write_odyssey_service() {
    cat <<EOF >>/etc/systemd/system/odyssey.service
[Unit]
Description=Run Odyssey Print Control Software
Requires=klipper.service
After=klipper.service

[Service]
ExecStart=${DIR}/odyssey --config ${CONFIG_DEST}
WorkingDirectory=${DIR}
Restart=always
RestartSec=10
Type=simple

[Install]
WantedBy=multi-user.target
EOF
}

install_service() {
    write_odyssey_service
    systemctl daemon-reload
    systemctl enable odyssey.service
    systemctl start odyssey.service
}

require_root() {
  if [[ $EUID -ne 0 ]]; then
    if ! command -v sudo >/dev/null 2>&1; then
      printf '\n[%s] This script must be run as root or via sudo.\n' "$SCRIPT_NAME" >&2
      exit 1
    fi
    printf '\n[%s] Elevating privileges with sudo...\n' "$SCRIPT_NAME"
    exec sudo -E bash "$0" "$@"
  fi
}

main() {
    require_root "$@"
    parse_args "$@"
    download_odyssey
    if [[ -z "$CREATE_SERVICE" ]]; then
        install_service
    fi
}

main "$@"
