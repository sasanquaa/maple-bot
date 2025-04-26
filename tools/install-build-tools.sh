#!/bin/bash

set -xe

root=$HOME/.build-tools
if [[ ! -d $root || -z $(ls -A $root) ]]; then
    rm -rf $root
    mkdir -p $root
    cd $root
    wget https://apt.llvm.org/llvm.sh
    wget https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh
    wget https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh
    chmod +x llvm.sh
    chmod +x install.sh
    chmod +x install-from-binstall-release.sh
fi

cd $root

sudo apt install -y cmake
sudo apt install -y ninja-build
sudo apt install -y protobuf-compiler
sudo ./llvm.sh 20 all
./install-from-binstall-release.sh
./install.sh
source ~/.nvm/nvm.sh
nvm install node
nvm use node
cargo binstall -y dioxus-cli
