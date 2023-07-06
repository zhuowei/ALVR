#!/bin/bash
set -e
#rm -r miniserver.o ../alvr/server/cpp/ios_prebuilt || true
xcrun -sdk xrsimulator clang -o miniserver -Os -Wall -target arm64-apple-xros1.0-simulator -framework Foundation -framework CoreFoundation -framework Security -framework AudioToolbox -framework Metal miniserver.cpp ../target/aarch64-apple-ios/debug/libalvr_server.a 2>&1|grep -v "built for iOS"
#mkdir -p ../alvr/server/cpp/ios_prebuilt
#ar rcs ../alvr/server/cpp/ios_prebuilt/libalvr_ios_prebuilt.a miniserver.o
