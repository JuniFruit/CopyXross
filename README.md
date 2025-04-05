# CopyXross App

A small and simple tool Copy/Paste string or files across machines in your local network. Available for both MacOs and Windows. 

### Usage

Simply run the app and start Copy/Pasting stuff around. Use Ctrl + C on one machine and use app UI menu to paste on another machine.

### Stuff to improve upon

Currently only single files are allowed to copy. If you need to copy folder, you have to compress it before. Copying files is no different from regular text. Just hit Ctrl + C on src machine and paste it on destination one.

No user notifications are implemented in case of errors. Currently only log file is available. For Windows it's in `~AppData/Roaming` and for Mac `~Library/Logs`.

MSI installer is not yet implemented. So only folder download is available.

### Installation

MacOs bundle is available here: [Google Drive](https://drive.google.com/file/d/14NCVNY7DWdmKWTky7W32Ju3KdOHAx2tR/view?usp=sharing). 
Windows build is here: [Google Drive](https://drive.google.com/file/d/1jaRISSCX-O7P3YFBQ3r-U8kob8ieGtd0/view?usp=sharing).

To build from source:

```
git clone https://github.com/JuniFruit/CopyXross.git
cd CopyXross
```
For Windows, it's a simple build cmd:
```
cargo build --release

```
Go to target/release and there you will find executable

For Mac we use bundler:
```
cargo install cargo-bundle
```
then
```
sudo cargo bundle --release
```
You will find your bundle in target/release/bundle/osx


