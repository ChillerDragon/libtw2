#!/bin/bash

SO_NAME=libwireshark_dissector.so
RELEASE_BIN="../target/release/$SO_NAME"
INSTALL_DIR="$HOME/.local/lib/wireshark/plugins/3.4/epan/"
INSTALL_BIN="$INSTALL_DIR/$SO_NAME"

mkdir -p "$INSTALL_DIR"


for arg in "$@"
do
    if [ "$arg" == "--help" ] || [ "$arg" == "-h" ] || [ "$arg" == "help" ]
    then
        echo "--uninstall"
        exit 0
    elif [ "$arg" == "--uninstall" ]
    then
        if [ -f "$INSTALL_BIN" ]
        then
            echo "uninstalling ..."
            rm "$INSTALL_BIN"
        else
            echo "not installed"
        fi
        exit 0
    fi
done

cargo build --release || exit 1
cp "$RELEASE_BIN" "$INSTALL_BIN"

