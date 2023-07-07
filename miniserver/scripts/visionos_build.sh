#!/bin/bash
# https://github.com/kewlbear/x264-ios/blob/master/build-x264.sh
cflags="-target arm64-apple-xros1.0-simulator -isysroot $(xcrun -sdk xrsimulator --show-sdk-path)"
../configure \
	--host=aarch64-apple-darwin \
	--prefix="$PWD/../out_visionos" \
        --extra-cflags="$cflags" \
        --extra-asflags="$cflags" \
        --extra-ldflags="$cflags" \
	--enable-static \
	--enable-pic \
	--disable-cli
