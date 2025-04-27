#!/bin/bash

set -xe

cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")"

root=$(readlink -f "..")/.cargo
opencv_root=$HOME/.opencv/install
msvc_root=$HOME/.msvc/msvc/bin/x64
if [[ ! -d $opencv_root || -z "$(ls -A $opencv_root)" ]]; then
    echo "This will still continue but you should run install-opencv.sh"
fi
if [[ ! -d $msvc_root || -z "$(ls -A $msvc_root)" ]]; then
    echo "This will still continue but you should run install-msvc.sh"
fi


rm -fr $root
mkdir -p $root
touch $root/config.toml
cat >$root/config.toml <<EOL
[env]
OPENCV_LINK_LIBS = "static=static=opencv_core4110,static=static=opencv_highgui4110,static=static=opencv_imgproc4110,static=static=opencv_dnn4110,static=static=opencv_imgcodecs4110,static=static=IlmImf,static=static=ippiw,static=static=libjpeg-turbo,static=static=libpng,static=static=libtiff,static=static=zlib,static=static=ippicvmt,static=static=ittnotify,static=static=libopenjp2,static=static=libprotobuf,static=static=libwebp"
OPENCV_LINK_PATHS = "$opencv_root/lib,$opencv_root/lib/opencv4/3rdparty"
OPENCV_INCLUDE_PATHS = "$opencv_root/include/opencv4"
OPENCV_MSVC_CRT = "static"
CC_x86_64_pc_windows_msvc = "$msvc_root/cl.exe"
CXX_x86_64_pc_windows_msvc = "$msvc_root/cl.exe"
AR_x86_64_pc_windows_msvc = "$msvc_root/lib.exe"

[target.x86_64-pc-windows-msvc]
linker = "$msvc_root/link.exe"

[build]
target = "x86_64-pc-windows-msvc"
EOL
