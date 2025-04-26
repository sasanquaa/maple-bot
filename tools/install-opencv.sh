#!/bin/bash

set -xe

cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")"

msvc_root=$HOME/.msvc/msvc
if [[ ! -d $msvc_root || -z "$(ls -A $msvc_root)" ]]; then
    echo "You should run install-msvc.sh first"
    exit 1
fi

root=$HOME/.opencv
root_src=$root/src
root_build=$root/build
root_install=$root/install
toolchain=$(readlink -f ".")/install-opencv-toolchain.cmake

if [[ -d $root_install && ! -z "$(ls -A $root_install)" ]]; then
    echo "OpenCV4 may have been already installed because the folder $root_install is not empty."
    exit 1
fi

rm -fr $root_build
mkdir -p $root_build
cd $root_build

git clone https://github.com/opencv/opencv $root_src || true
git -C $root_src checkout 4.11.0

cmake ../src \
    -GNinja \
    -DMSVC_ROOT=$msvc_root \
    -DCMAKE_TOOLCHAIN_FILE=$toolchain \
    -DCMAKE_INSTALL_PREFIX=$root_install \
    -DBUILD_LIST=core,dnn,highgui,imgproc,imgcodecs
ninja
ninja install
