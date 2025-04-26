#!/bin/bash

set -xe

root=$HOME/.msvc

if [[ ! -d $root || -z "$(ls -A $root)" ]]; then
    rm -fr $root
    mkdir -p $root
else
    exit 1
fi

sudo apt-get update
sudo apt-get install -y wine64 python3 msitools ca-certificates winbind
git clone https://github.com/mstorsjo/msvc-wine $root
cd $root
./vsdownload.py --dest ./msvc
./install.sh ./msvc
