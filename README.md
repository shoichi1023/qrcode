# qrcode

```sh
qrcode 0.1.0

USAGE:
    qrcode <SUBCOMMAND>

OPTIONS:
    -h, --help       Print help information
    -V, --version    Print version information

SUBCOMMANDS:
    generate       generate QR Code
    help           Print this message or the help of the given subcommand(s)
    img-replace    replace QR Code on image
    replace        replace QR Code on video

```

## build for mac
```sh
cargo build
cp ./target/debug/qrcode ./qrcode
```


## getting start for mac

```sh
brew install opencv
./qrcode -h
```

## build for windows on windows

### install chocolatey

```
Set-ExecutionPolicy Bypass -Scope Process -Force; [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072; iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
```

### install llvm and opencv and adding to path

```
choco install llvm opencv
setx OPENCV_INCLUDE_PATHS C:¥tools¥opencv¥build¥include
setx OPENCV_LINK_LIBS opencv_world455
setx OPENCV_LINK_LIB_PATHS C:¥tools¥opencv¥build¥x64¥vc15¥lib
```

and adding `C:¥tools¥opencv¥build¥x64¥vc¥15¥bin` in your path.

please reboot.

```
cargo build
mkdir bin
XCOPY .¥target¥debug¥qrcode.exe .¥bin¥qrcode.exe
XCOPY C:¥tools¥opencv¥build¥x64¥vc¥15¥bin¥* .¥bin
```

## getting start for windows

download zip file on release page

```
unzip win-x64-qrcode.zip
cd ./win-x64-qrcode/bin
qrcode -h
```