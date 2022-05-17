# Rolf

![Demo with image](demo.gif)

> This is _definitely_ a work in progress. Use at your own risk.

`rolf` is a terminal file manager written in Rust. It is mostly inspired by
`lf`, but it is far worse.

## Release Notes: 5/16/2022

### Additional Features

- File selection (by default mapped to the space key)
  - Press the space key (by default) to mark a file as selected,
    automatically moving the cursor to the next file.
  - Files will remain selected even if you change directories, allowing you to
    have files from multiple directories selected at once.
  - Edit the selected files with text editor via the 'edit-sels' command.
- File deletion (by default mapped to 'd')
  - This will prompt the user to confirm that they really want to delete the
    file(s).
  - If no files have been selected, this will delete the current file.
  - If files have been selected, this will delete the currently selected
    files.

### Known Bugs

- There is a slight amount of flickering when using the 'rename' command.
- The program may crash with really small windows.

## TODOs
- [x] Render three panels
- [x] Support super-basic hjkl movement
- [x] Allow key rebindings
- [x] Use [highlight](http://www.andre-simon.de/doku/highlight/highlight.php)
      for text file highlighting
- [x] Make [highlight](http://www.andre-simon.de/doku/highlight/highlight.php)
      an optional dependency.
- [x] Implement part of command-line
  - [x] Implement some basic GNU readline keybindings
- [x] EXPERIMENTAL: Display image previews on kitty
  - [x] Display images asynchronously
  - [x] Show image thumbnail for videos
  - [ ] Allow a configurable external program to be used to obtain preview
        images for (non-image) files
- [x] Make symlinks actually usable
- [x] Fix compiler warnings on latest version of Rust stable
- [x] Search backwards and forwards
- [x] Display directory preview asynchronously
- [x] Preview text (source) files asynchronously
- [x] Build successfully on Windows at all
- [x] Display images successfully on Windows (with capable terminal)

# Long-term TODO (May Never Happen)
- [ ] Add Windows support
- [ ] Implement our own png and jpeg decoders
- [x] Use the kitty protocol (and possibly other terminal image protocols)
      directly for image previews
