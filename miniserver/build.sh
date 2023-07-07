#!/bin/bash
set -e
xcrun -sdk xrsimulator clang++ -o miniserver -Os -Wall -target arm64-apple-xros1.0-simulator -std=c++17 -g \
	-framework Foundation -framework CoreFoundation -framework Security -framework AudioToolbox -framework Metal \
	-I/Volumes/orangehd/docs/repos/x264/out_visionos/include \
	miniserver.cpp EncodePipelineSW.cpp NalParsing.cpp \
	../target/aarch64-apple-ios/debug/libalvr_server.a \
	/Volumes/orangehd/docs/repos/x264/out_visionos/lib/libx264.a \
	2>&1|grep -v "built for iOS"
