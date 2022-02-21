# Rolf

![Demo with image](demo.gif)

> This is _definitely_ a work in progress. Use at your own risk.

`rolf` is a terminal file manager written in Rust. It is mostly inspired by
`lf`, but it is far worse.

## Documentation
Unfortunately, there's no documentation.

Once `rolf` is in a more usable state, I'll actually add something useful.

## TODOs
- [x] Render three panels
- [x] Support super-basic hjkl movement
- [ ] Allow key rebindings
- [x] Use [highlight](http://www.andre-simon.de/doku/highlight/highlight.php)
      for text file highlighting
- [x] Make [highlight](http://www.andre-simon.de/doku/highlight/highlight.php)
      an optional dependency.
- [x] Implement part of command-line
  - [x] Implement some basic GNU readline keybindings
- [x] EXPERIMENTAL: Display image previews on kitty
  - [x] Display images asynchronously
  - [x] Show image thumbnail for videos
  - [ ] Make ffmpeg an optional dependency
- [x] Make symlinks actually usable
- [x] Fix compiler warnings on latest version of Rust stable
- [x] Search backwards and forwards
- [x] Display directory preview asynchronously
- [x] Preview text (source) files asynchronously
- [x] Build successfully on Windows at all
- [ ] Display images successfully on Windows (with capable terminal)

# Long-term TODO (May Never Happen)
- [ ] Add Windows support
- [ ] Implement our own png and jpeg decoders
- [x] Use the kitty protocol (and possibly other terminal image protocols)
      directly for image previews
- [ ] Make the file manager way faster
  - [ ] Profile passing DrawingInfo (and possibly ColumnInfo) by reference or
        by copy
- [ ] Refactor variables into structs, to avoid having massive numbers of
      parameters to functions
