#!/bin/bash

set -xe

root=$HOME/.llvm-installer
if [[ -d $root && ! -z $(ls -A $root) ]]; then
    exit 1
fi

rm -rf $root
mkdir -p $root
cd $root
wget https://apt.llvm.org/llvm.sh
chmod +x llvm.sh
sudo ./llvm.sh 20 all

