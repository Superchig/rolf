// FIXME(Chris): Remove the dead_code warning suppressions
#![allow(dead_code, unused_macros, unused_variables, unused_imports)]
#![allow(
    clippy::absurd_extreme_comparisons,
    clippy::too_many_arguments,
    clippy::never_loop
)]

mod natural_sort; // This declares the existence of the natural_sort module, which searches by
                  // default for natural_sort.rs or natural_sort/mod.rs

mod config;
mod human_size;
mod line_edit;
mod os_abstract;
#[cfg(unix)]
mod strmode;
mod tiff;
#[cfg(unix)]
mod unix_users;

use config::{Config, ImageProtocol};
use human_size::human_size;
use image::png::PngEncoder;
use natural_sort::cmp_natural;
use os_abstract::WindowPixels;
use tiff::{usizeify, Endian, EntryTag, EntryType, IFDEntry};

#[cfg(unix)]
use strmode::strmode;
use which::which;

use std::cmp::Ordering;
use std::collections::hash_map::HashMap;
use std::env;
use std::fs::{self, DirEntry, Metadata};
use std::io::{self, BufRead, BufReader, BufWriter, StdoutLock, Write};
use std::path::{self, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::vec::Vec;

use image::{ColorType, GenericImageView, ImageBuffer, ImageEncoder, Rgba};

use tokio::runtime::{Builder, Runtime};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute, queue,
    style::{self, Attribute, Color},
    terminal::{self, ClearType},
};

use rolf_grid::{LineBuilder, Style};
use rolf_parser::parser::{parse, parse_statement_from, Program, Statement};

type Screen = rolf_grid::Screen<io::Stdout>;

// TODO(Chris): Make this configurable rather than hard-coding the constant
const SCROLL_OFFSET: u16 = 10;

type HandlesVec = Vec<DrawHandle>;
type SelectionsMap = HashMap<PathBuf, usize>;

fn main() -> crossterm::Result<()> {
    let mut w = io::stdout();

    let args: Vec<String> = std::env::args().collect();

    let mut last_dir_path = None;

    for (index, arg) in args.iter().enumerate() {
        if arg == "-last-dir-path" {
            if args.len() - 1 <= index + 1 {
                last_dir_path = Some(PathBuf::from(args[index + 1].clone()));
            } else {
                // TODO(Chris): Show a better startup error
                return Err(io::Error::from(io::ErrorKind::InvalidInput));
            }
        }
    }

    let mut config = match fs::read_to_string("config.json") {
        Ok(json) => config::parse_config(&json),
        Err(err) => match err.kind() {
            io::ErrorKind::NotFound => Config::default(),
            _ => panic!("Error opening config file: {}", err),
        },
    };

    let term = env::var("TERM").unwrap_or_default();

    if config.image_protocol == ImageProtocol::Auto {
        if config::check_iterm_support() {
            config.image_protocol = ImageProtocol::ITerm2;
        } else if term == "xterm-kitty" {
            config.image_protocol = ImageProtocol::Kitty;
        } else {
            config.image_protocol = ImageProtocol::None;
        }
    }

    let ast = match fs::read_to_string("rolfrc") {
        Ok(config_text) => {
            // FIXME(Chris): Handle error here
            parse(&config_text).unwrap()
        }
        Err(err) => match err.kind() {
            io::ErrorKind::NotFound => vec![],
            _ => panic!("Error opening config file: {}", err),
        },
    };

    Screen::activate_direct(&mut w)?;

    let result = run(&mut config, &ast);

    Screen::deactivate_direct(&mut w)?;

    match result {
        Ok(current_dir) => {
            if let Some(last_dir_path) = last_dir_path {
                std::fs::write(last_dir_path, current_dir.to_str().unwrap()).unwrap()
            }
        }
        Err(err) => panic!("{}", err),
    }

    Ok(())
}

// Returns the path to the last dir
fn run(_config: &mut Config, config_ast: &Program) -> crossterm::Result<PathBuf> {
    let user_name = whoami::username();

    let host_name = whoami::hostname();

    let home_name = os_abstract::get_home_name();

    let home_path = Path::new(&home_name[..]);

    // NOTE(Chris): The default column ratio is 1:2:3

    let mut fm = FileManager {
        runtime: Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap(),

        available_execs: {
            let mut available_execs: HashMap<&str, std::path::PathBuf> = HashMap::new();

            insert_executable(&mut available_execs, "highlight");

            insert_executable(&mut available_execs, "ffmpeg");

            available_execs
        },

        image_handles: vec![],

        dir_states: DirStates::new()?,

        second: ColumnInfo {
            starting_index: 0,
            display_offset: 0,
        },

        left_paths: HashMap::new(),

        match_positions: vec![],

        should_search_forwards: true,

        input_line: String::new(),

        input_cursor: 0,

        input_mode: InputMode::Normal,

        user_host_display: format!("{}@{}", user_name, host_name),

        // Keys are paths, values are indices in their directory
        selections: HashMap::new(),

        drawing_info: DrawingInfo {
            win_pixels: os_abstract::get_win_pixels()?,
            width: 0,
            height: 0,
            column_bot_y: 0,
            column_height: 0,
            first_left_x: 0,
            first_right_x: 0,
            second_left_x: 0,
            second_right_x: 0,
            third_left_x: 0,
            third_right_x: 0,
        },

        config: _config.clone(),

        preview_data: PreviewData::Loading,
    };

    update_drawing_info_from_resize(&mut fm.drawing_info)?;

    let screen = Screen::new(io::stdout())?;
    let screen = Mutex::new(screen);

    let mut command_queue = config_ast.clone();

    let (tx, rx) = channel();

    let crossterm_input_tx = tx.clone();

    // Crossterm input loop
    std::thread::spawn(move || loop {
        let crossterm_event = event::read().expect("Unable to read crossterm event");

        crossterm_input_tx
            .send(InputEvent::CrosstermEvent(crossterm_event))
            .expect("Unable to send on channel");
    });

    let mut prev_dir_display = String::new();
    let mut prev_second_entry_index = 0;

    // Main input loop
    'input: loop {
        let second_entry_index = fm.second.starting_index + fm.second.display_offset;

        let second_bottom_index = fm.second.starting_index + fm.drawing_info.column_height;

        for stm in &command_queue {
            match stm {
                Statement::Map(map) => {
                    let key_event = config::to_key(&map.key.key);
                    fm.config
                        .keybindings
                        .insert(key_event, map.cmd_name.clone());
                }
                Statement::CommandUse(command_use) => {
                    let command: &str = &command_use.name;

                    match command {
                        "quit" => {
                            break 'input;
                        }
                        "down" => {
                            if !fm.dir_states.current_entries.is_empty()
                                && (second_entry_index as usize)
                                    < fm.dir_states.current_entries.len() - 1
                            {
                                abort_image_handles(&mut fm.image_handles);

                                if fm.second.display_offset
                                    >= (fm.drawing_info.column_height - SCROLL_OFFSET - 1)
                                    && (second_bottom_index as usize)
                                        < fm.dir_states.current_entries.len()
                                {
                                    fm.second.starting_index += 1;
                                } else if second_entry_index < second_bottom_index {
                                    fm.second.display_offset += 1;
                                }
                            }
                        }
                        "up" => {
                            if !fm.dir_states.current_entries.is_empty() {
                                abort_image_handles(&mut fm.image_handles);

                                if fm.second.display_offset <= (SCROLL_OFFSET)
                                    && fm.second.starting_index > 0
                                {
                                    fm.second.starting_index -= 1;
                                } else if second_entry_index > 0 {
                                    fm.second.display_offset -= 1;
                                }
                            }
                        }
                        "updir" => {
                            abort_image_handles(&mut fm.image_handles);

                            let old_current_dir = fm.dir_states.current_dir.clone();
                            if !fm.dir_states.current_entries.is_empty() {
                                save_location(&mut fm, second_entry_index);
                            }

                            if let Some(parent_dir) = fm.dir_states.prev_dir.clone() {
                                set_current_dir(
                                    parent_dir,
                                    &mut fm.dir_states,
                                    &mut fm.match_positions,
                                )?;
                            }

                            fm.second = find_correct_location(
                                &fm.left_paths,
                                fm.drawing_info.column_height,
                                &fm.dir_states.current_dir,
                                &fm.dir_states.current_entries,
                                &old_current_dir,
                            );
                        }
                        "open" => {
                            enter_entry(&mut fm, second_entry_index)?;
                        }
                        // NOTE(Chris): lf doesn't actually provide a specific command for this, instead using
                        // a default keybinding that takes advantage of EDITOR
                        "edit" => {
                            let editor = match std::env::var("VISUAL") {
                                Err(std::env::VarError::NotPresent) => {
                                    match std::env::var("EDITOR") {
                                        Err(std::env::VarError::NotPresent) => String::from(""),
                                        Err(err) => panic!("{}", err),
                                        Ok(editor) => editor,
                                    }
                                }
                                Err(err) => panic!("{}", err),
                                Ok(visual) => visual,
                            };

                            // It'd be nice if we could do breaking on blocks to exit this whole
                            // match statement early, but labeling blocks is still in unstable,
                            // as seen in https://github.com/rust-lang/rust/issues/48594
                            if !editor.is_empty() {
                                let selected_entry =
                                    &fm.dir_states.current_entries[second_entry_index as usize];

                                let shell_command = format!(
                                    "{} {}",
                                    editor,
                                    selected_entry
                                        .dir_entry
                                        .path()
                                        .to_str()
                                        .expect("Failed to convert path to string")
                                );

                                Command::new("sh")
                                    .arg("-c")
                                    .arg(shell_command)
                                    .status()
                                    .expect("failed to execute editor command");
                            }
                        }
                        "top" => {
                            if !fm.dir_states.current_entries.is_empty() {
                                abort_image_handles(&mut fm.image_handles);

                                fm.second.starting_index = 0;
                                fm.second.display_offset = 0;
                            }
                        }
                        "bottom" => {
                            if !fm.dir_states.current_entries.is_empty() {
                                abort_image_handles(&mut fm.image_handles);

                                if fm.dir_states.current_entries.len()
                                    <= (fm.drawing_info.column_height as usize)
                                {
                                    fm.second.starting_index = 0;
                                    fm.second.display_offset =
                                        fm.dir_states.current_entries.len() as u16 - 1;
                                } else {
                                    fm.second.display_offset = fm.drawing_info.column_height - 1;
                                    fm.second.starting_index = fm.dir_states.current_entries.len()
                                        as u16
                                        - fm.second.display_offset
                                        - 1;
                                }
                            }
                        }
                        "search" => {
                            assert!(fm.input_line.len() <= 0);

                            fm.input_line.push_str("search ");

                            fm.input_mode = InputMode::Command;
                        }
                        "search-back" => {
                            assert!(fm.input_line.len() <= 0);

                            fm.input_line.push_str("search-back ");

                            fm.input_mode = InputMode::Command;
                        }
                        "search-next" => {}
                        "search-prev" => {}
                        "toggle" => {
                            let selected_entry =
                                &fm.dir_states.current_entries[second_entry_index as usize];

                            let entry_path = selected_entry.dir_entry.path();

                            let remove = fm.selections.remove(&entry_path);
                            if remove.is_none() {
                                fm.selections
                                    .insert(entry_path, second_entry_index as usize);
                            }
                        }
                        "read" => {
                            fm.input_mode = InputMode::Command;
                        }
                        _ => (),
                    }
                }
            }
        }

        command_queue.clear();

        // Main drawing code
        {
            // NOTE(Chris): Recompute second_entry_index since the relevant values may have
            // been modified
            let second_entry_index = fm.second.starting_index + fm.second.display_offset;

            let current_dir_display = format_current_dir(&fm.dir_states, home_path);

            let has_changed_entry = current_dir_display != prev_dir_display
                || second_entry_index != prev_second_entry_index;
            prev_dir_display.clone_from(&current_dir_display);
            prev_second_entry_index = second_entry_index;

            let curr_entry;
            let file_stem = if fm.dir_states.current_entries.len() <= 0 {
                ""
            } else {
                curr_entry = fm.dir_states.current_entries[second_entry_index as usize]
                    .dir_entry
                    .file_name();
                curr_entry.to_str().unwrap()
            };

            // TODO(Chris): Use the unicode-segmentation package to count graphemes
            // Add 1 because of the ':' that is displayed after user_host_display
            // Add 1 again because of the '/' that is displayed at the end of current_dir_display
            let remaining_width = fm.drawing_info.width as usize
                - (fm.user_host_display.len() + 1 + current_dir_display.len() + 1);

            let file_stem = if file_stem.len() > remaining_width {
                String::from(&file_stem[..remaining_width])
            } else {
                String::from(file_stem)
            };

            let mut screen_lock = screen.lock().expect("Failed to lock screen mutex!");
            let screen_lock = &mut *screen_lock;
            screen_lock.clear_logical();

            let user_host_len = fm.user_host_display.len().try_into().unwrap();
            draw_str(
                screen_lock,
                0,
                0,
                &fm.user_host_display,
                rolf_grid::Style::new(
                    rolf_grid::Attribute::Bold,
                    rolf_grid::Color::Green,
                    rolf_grid::Color::Background,
                ),
            );
            draw_str(
                screen_lock,
                user_host_len,
                0,
                ":",
                rolf_grid::Style::default(),
            );
            draw_str(
                screen_lock,
                user_host_len + 1, // From the ":"
                0,
                &format!("{}{}", current_dir_display, path::MAIN_SEPARATOR),
                rolf_grid::Style::new(
                    rolf_grid::Attribute::Bold,
                    rolf_grid::Color::Blue,
                    rolf_grid::Color::Background,
                ),
            );
            draw_str(
                screen_lock,
                user_host_len + 1 + current_dir_display.len() as u16 + 1,
                0,
                &file_stem,
                rolf_grid::Style::new(
                    rolf_grid::Attribute::Bold,
                    rolf_grid::Color::Foreground,
                    rolf_grid::Color::Background,
                ),
            );

            draw_first_column(screen_lock, &mut fm);

            // FIXME(Chris): Refactor this into FileManager or DrawingInfo
            let second_column_rect = Rect {
                left_x: fm.drawing_info.second_left_x,
                top_y: 1,
                width: fm.drawing_info.second_right_x - fm.drawing_info.second_left_x,
                height: fm.drawing_info.column_height,
            };

            draw_column(
                screen_lock,
                second_column_rect,
                fm.second.starting_index,
                second_entry_index,
                &fm.dir_states.current_entries,
            );

            let third_column_rect = Rect {
                left_x: fm.drawing_info.third_left_x,
                top_y: 1,
                width: fm.drawing_info.third_right_x - fm.drawing_info.third_left_x,
                height: fm.drawing_info.column_height,
            };

            if has_changed_entry {
                for x in fm.drawing_info.third_left_x..=fm.drawing_info.width - 1 {
                    for y in 1..=fm.drawing_info.column_bot_y {
                        screen_lock.set_dead(x, y, false);
                    }
                }

                match fm.config.image_protocol {
                    ImageProtocol::Kitty => {
                        // https://sw.kovidgoyal.net/kitty/graphics-protocol/#deleting-images
                        let mut w = io::stdout();
                        w.write_all(b"\x1b_Ga=d;\x1b\\")?; // Delete all visible images
                    }
                    ImageProtocol::ITerm2 => {
                        // NOTE(Chris): We don't actually need to do anything here, it seems
                    }
                    _ => (),
                }
            }

            if !fm.dir_states.current_entries.is_empty() {
                // NOTE(Chris): We keep this code block before the preview drawing
                // functionality in order to properly set up the Loading... message.
                if has_changed_entry {
                    set_preview_data_with_thread(&mut fm, &tx, second_entry_index);
                }

                match &fm.preview_data {
                    PreviewData::Loading => {
                        draw_str(
                            screen_lock,
                            third_column_rect.left_x + 2,
                            third_column_rect.top_y,
                            "Loading...",
                            Style::new_attr(rolf_grid::Attribute::Reverse),
                        );
                    }
                    PreviewData::Blank => (),
                    PreviewData::Directory { entries_info } => {
                        let third_dir = &fm.dir_states.current_entries[second_entry_index as usize]
                            .dir_entry
                            .path();

                        let (display_offset, starting_index) = match fm.left_paths.get(third_dir) {
                            Some(dir_location) => {
                                (dir_location.display_offset, dir_location.starting_index)
                            }
                            None => (0, 0),
                        };

                        let entry_index = starting_index + display_offset;

                        draw_column(
                            screen_lock,
                            third_column_rect,
                            starting_index,
                            entry_index,
                            entries_info,
                        );
                    }
                    PreviewData::UncoloredFile { path } => {
                        // TODO(Chris): Handle permission errors here
                        let file = fs::File::open(path)?;
                        let reader = BufReader::new(file);

                        let draw_style = rolf_grid::Style::default();

                        let inner_left_x = fm.drawing_info.third_left_x + 2;

                        // NOTE(Chris): 1 is the top_y for all columns
                        let mut curr_y = 1;

                        let third_width = fm.drawing_info.third_right_x - inner_left_x;

                        for line in reader.lines() {
                            // TODO(Chris): Handle UTF-8 errors here, possibly by just
                            // showing an error line
                            let line = match line {
                                Ok(line) => line,
                                Err(_) => break,
                            };

                            if curr_y > fm.drawing_info.column_bot_y {
                                break;
                            }

                            if line.len() < (third_width as usize) {
                                draw_str(screen_lock, inner_left_x, curr_y, &line, draw_style);
                            } else {
                                draw_str(
                                    screen_lock,
                                    inner_left_x,
                                    curr_y,
                                    &line[0..third_width as usize],
                                    draw_style,
                                );
                            }

                            curr_y += 1;
                        }
                    }
                    PreviewData::ImageBuffer { buffer } => {
                        match fm.config.image_protocol {
                            ImageProtocol::Kitty => {
                                let raw_img = buffer.as_raw();

                                let stdout = io::stdout();
                                let mut w = stdout.lock();

                                let path = store_in_tmp_file(raw_img)?;

                                queue!(
                                    w,
                                    cursor::MoveTo(fm.drawing_info.third_left_x, 1),
                                    // Hide the "Should display!" / "Loading..." message
                                    style::Print("               "),
                                    cursor::MoveTo(fm.drawing_info.third_left_x, 1),
                                )?;

                                write!(
                                    w,
                                    "\x1b_Gf=32,s={},v={},a=T,t=t;{}\x1b\\",
                                    buffer.width(),
                                    buffer.height(),
                                    base64::encode(path.to_str().unwrap())
                                )?;

                                w.flush()?;

                                for x in fm.drawing_info.third_left_x..=fm.drawing_info.width - 1 {
                                    for y in 1..=fm.drawing_info.column_bot_y {
                                        screen_lock.set_dead(x, y, true);
                                    }
                                }
                            }
                            _ => {
                                panic!("Unsupported image protocol: {:?}", fm.config.image_protocol)
                            }
                        }
                    }
                    PreviewData::RawBytes { bytes } => {
                        let stdout = io::stdout();
                        let mut w = stdout.lock();

                        let inner_left_x = fm.drawing_info.third_left_x + 2;

                        queue!(
                            w,
                            cursor::MoveTo(fm.drawing_info.third_left_x, 1),
                            // Hide the "Should display!" / "Loading..." message
                            style::Print("               "),
                            cursor::MoveTo(fm.drawing_info.third_left_x, 1),
                        )?;

                        let mut curr_y = 1; // Columns start at y = 1
                        queue!(&mut w, cursor::MoveTo(inner_left_x, curr_y))?;

                        queue!(&mut w, terminal::DisableLineWrap)?;

                        for ch in bytes {
                            if curr_y > fm.drawing_info.column_bot_y {
                                break;
                            }

                            if *ch == b'\n' {
                                curr_y += 1;

                                queue!(&mut w, cursor::MoveTo(inner_left_x, curr_y))?;
                            } else {
                                // NOTE(Chris): We write directly to stdout so as to
                                // allow the ANSI escape codes to match the end of a
                                // line
                                w.write_all(&[*ch])?;
                            }
                        }

                        queue!(&mut w, terminal::EnableLineWrap)?;

                        // TODO(Chris): Refactor this into a function, since it's
                        // used three times (if you include the modification of the
                        // set_dead bool)
                        for x in fm.drawing_info.third_left_x..=fm.drawing_info.width - 1 {
                            for y in 1..=fm.drawing_info.column_bot_y {
                                screen_lock.set_dead(x, y, true);
                            }
                        }
                    }
                }
            }

            match fm.input_mode {
                InputMode::Normal => {
                    draw_bottom_info_line(screen_lock, &mut fm);

                    screen_lock.hide_cursor();
                }
                InputMode::Command => {
                    screen_lock.set_cell(0, fm.drawing_info.height - 1, ':');

                    draw_str(
                        screen_lock,
                        1, // To make room for ':'
                        fm.drawing_info.height - 1,
                        &fm.input_line,
                        rolf_grid::Style::default(),
                    );

                    screen_lock.show_cursor(
                        (fm.input_cursor + 1).try_into().unwrap(),
                        fm.drawing_info.height - 1,
                    );
                }
            }

            screen_lock.show()?;
        }

        let event = rx.recv().unwrap();

        match event {
            InputEvent::CrosstermEvent(event) => match event {
                Event::Key(event) => {
                    match fm.input_mode {
                        InputMode::Normal => {
                            if let Some(bound_command) = fm.config.keybindings.get(&event) {
                                // FIXME(Chris): Handle the possible error here
                                command_queue.push(parse_statement_from(bound_command).unwrap());
                            }
                        }
                        InputMode::Command => match event.code {
                            KeyCode::Esc => {
                                fm.input_mode = InputMode::Normal;

                                fm.input_line.clear();
                                fm.input_cursor = 0;
                            }
                            KeyCode::Char(ch) => {
                                fm.input_line.insert(fm.input_cursor, ch);
                                fm.input_cursor += 1;
                            }
                            KeyCode::Enter => {
                                if let Ok(stm) = parse_statement_from(&fm.input_line) {
                                    command_queue.push(stm);
                                }

                                fm.input_mode = InputMode::Normal;

                                fm.input_line.clear();
                                fm.input_cursor = 0;
                            }
                            KeyCode::Left => {
                                if fm.input_cursor > 0 {
                                    fm.input_cursor -= 1;
                                }
                            }
                            KeyCode::Right => {
                                if fm.input_cursor < fm.input_line.len() {
                                    fm.input_cursor += 1;
                                }
                            }
                            KeyCode::Backspace => {
                                if fm.input_cursor > 0 {
                                    fm.input_line.remove(fm.input_cursor - 1);
                                    
                                    fm.input_cursor -= 1;
                                }
                            }
                            _ => (),
                        },
                    }
                }
                Event::Mouse(_) => (),
                Event::Resize(width, height) => {
                    let mut screen_lock = screen.lock().expect("Failed to lock screen mutex!");
                    let screen_lock = &mut *screen_lock;

                    screen_lock.resize_clear_draw(width, height)?;

                    update_drawing_info_from_resize(&mut fm.drawing_info)?;
                }
            },
            InputEvent::PreviewLoaded(preview_data) => {
                fm.preview_data = preview_data;
            }
        }
    }

    Ok(fm.dir_states.current_dir)
}

struct FileManager<'a> {
    runtime: Runtime,

    available_execs: HashMap<&'a str, std::path::PathBuf>,

    image_handles: HandlesVec,

    dir_states: DirStates,

    second: ColumnInfo,

    left_paths: HashMap<std::path::PathBuf, DirLocation>,

    match_positions: Vec<usize>,

    should_search_forwards: bool,

    input_line: String,

    input_cursor: usize,

    input_mode: InputMode,

    user_host_display: String,

    selections: SelectionsMap,

    drawing_info: DrawingInfo,

    config: Config,

    preview_data: PreviewData,
}

enum InputMode {
    Normal,
    Command,
}

// NOTE(Chris): When it comes to refactoring many variables into structs, perhaps we should group
// them by when they are modified. For example, DrawingInfo is modified whenever the terminal
// window resizes, while ColumnInfo will be modified even when the terminal window isn't resizing.
// Thus, we should maybe put the left_x value for each column in DrawingInfo (rather than
// ColumnInfo), since those will primarily be modified when the terminal window changes.

#[derive(Clone, Copy)]
struct DrawingInfo {
    win_pixels: WindowPixels,
    width: u16,
    height: u16,
    column_bot_y: u16,
    column_height: u16,
    first_right_x: u16,
    first_left_x: u16,
    second_left_x: u16,
    second_right_x: u16,
    third_left_x: u16,
    third_right_x: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ColumnInfo {
    starting_index: u16,
    display_offset: u16,
}

enum InputEvent {
    CrosstermEvent(crossterm::event::Event),
    PreviewLoaded(PreviewData),
}

fn set_preview_data_with_thread(
    fm: &mut FileManager,
    tx: &Sender<InputEvent>,
    second_entry_index: u16,
) {
    let second_entry = &fm.dir_states.current_entries[second_entry_index as usize];

    fm.preview_data = PreviewData::Loading;

    let third_file_path = second_entry.dir_entry.path();

    match second_entry.file_type {
        // TODO(Chris): Optimize entry gathering to avoid spawning a thread if there's a low (<
        // 200) number of entries, without reading in entries twice
        RecordedFileType::Directory | RecordedFileType::DirectorySymlink => {
            let (can_draw_clone, preview_tx) = clone_thread_helpers(fm, tx);

            let preview_image_handle = std::thread::spawn(move || {
                let preview_entry_info = get_sorted_entries(&third_file_path).unwrap();

                let len = preview_entry_info.len();

                let can_display = can_draw_clone.load(std::sync::atomic::Ordering::Acquire);

                if can_display {
                    preview_tx
                        .send(InputEvent::PreviewLoaded(PreviewData::Directory {
                            entries_info: preview_entry_info,
                        }))
                        .expect("Unable to send on channel");
                }
            });
        }
        RecordedFileType::File | RecordedFileType::FileSymlink => {
            if let Some(os_str_ext) = third_file_path.extension() {
                if let Some(ext) = os_str_ext.to_str() {
                    let ext = ext.to_lowercase();
                    let ext = ext.as_str();

                    match ext {
                        "png" | "jpg" | "jpeg" | "mp4" | "webm" | "mkv" => {
                            let (can_draw_clone, preview_tx) = clone_thread_helpers(fm, tx);

                            let ext_string = ext.to_string();
                            let drawing_info = fm.drawing_info;
                            let image_protocol = fm.config.image_protocol;

                            let preview_image_handle = std::thread::spawn(move || {
                                let image_buffer = match preview_image_or_video(
                                    drawing_info.win_pixels,
                                    third_file_path,
                                    ext_string,
                                    drawing_info.width,
                                    drawing_info.height,
                                    drawing_info.third_left_x,
                                    image_protocol,
                                ) {
                                    Ok(image_buffer) => image_buffer,
                                    Err(_) => return,
                                };

                                let can_display_image =
                                    can_draw_clone.load(std::sync::atomic::Ordering::Acquire);

                                if can_display_image {
                                    preview_tx
                                        .send(InputEvent::PreviewLoaded(PreviewData::ImageBuffer {
                                            buffer: image_buffer,
                                        }))
                                        .expect("Unable to send on channel");
                                }
                            });
                        }
                        _ => match fm.available_execs.get("highlight") {
                            None => {
                                fm.preview_data = PreviewData::UncoloredFile {
                                    path: third_file_path,
                                };
                            }
                            Some(highlight) => {
                                let highlight = highlight.clone();

                                let (can_draw_clone, preview_tx) = clone_thread_helpers(fm, tx);

                                std::thread::spawn(move || {
                                    // TODO(Chris): Actually show that something went wrong
                                    let output = Command::new(highlight)
                                        .arg("-O")
                                        .arg("ansi")
                                        .arg("--max-size=500K")
                                        .arg(third_file_path)
                                        .output()
                                        .unwrap();

                                    preview_tx
                                        .send(InputEvent::PreviewLoaded(PreviewData::RawBytes {
                                            bytes: output.stdout,
                                        }))
                                        .expect("Unable to send on channel");
                                });
                            }
                        },
                    }
                } else {
                    fm.preview_data = PreviewData::UncoloredFile {
                        path: third_file_path,
                    };
                }
            } else {
                fm.preview_data = PreviewData::UncoloredFile {
                    path: third_file_path,
                };
            }
        }
        RecordedFileType::InvalidSymlink | RecordedFileType::Other => {
            fm.preview_data = PreviewData::Blank;
        }
    }
}

fn clone_thread_helpers(
    fm: &mut FileManager,
    tx: &Sender<InputEvent>,
) -> (Arc<AtomicBool>, Sender<InputEvent>) {
    let can_draw = Arc::new(AtomicBool::new(true));
    let can_draw_clone = Arc::clone(&can_draw);
    let preview_tx = tx.clone();

    fm.image_handles.push(DrawHandle { can_draw });

    (can_draw_clone, preview_tx)
}

fn draw_column(
    screen: &mut Screen,
    rect: Rect,
    file_top_ind: u16,
    file_curr_ind: u16,
    items: &[DirEntryInfo],
) {
    let inner_left_x = rect.left_x + 1;

    if items.is_empty() {
        draw_str(
            screen,
            inner_left_x + 1,
            rect.top_y,
            "empty",
            Style::new_attr(rolf_grid::Attribute::Reverse),
        );
    }

    // NOTE(Chris): 1 is the starting row for columns
    for y in rect.top_y..rect.bot_y() {
        let ind = file_top_ind + y - 1;

        if (ind as usize) >= items.len() {
            break;
        }

        let entry_info = &items[ind as usize];

        let mut draw_style = if ind == file_curr_ind {
            Style::new_attr(rolf_grid::Attribute::Reverse)
        } else {
            Style::new_attr(rolf_grid::Attribute::None)
        };

        match entry_info.file_type {
            RecordedFileType::Directory => {
                draw_style.fg = rolf_grid::Color::Blue;
                draw_style.attribute |= rolf_grid::Attribute::Bold;
            }
            RecordedFileType::FileSymlink | RecordedFileType::DirectorySymlink => {
                draw_style.fg = rolf_grid::Color::Cyan;
                draw_style.attribute |= rolf_grid::Attribute::Bold;
            }
            RecordedFileType::InvalidSymlink => {
                draw_style.fg = rolf_grid::Color::Red;
                draw_style.attribute |= rolf_grid::Attribute::Bold;
            }
            _ => (),
        }

        let file_name_os = entry_info.dir_entry.file_name();

        let file_name = file_name_os.to_str().unwrap();

        screen.set_cell_style(inner_left_x, y, ' ', draw_style);
        let name_pos_x = inner_left_x + 1;
        draw_str(screen, name_pos_x, y, file_name, draw_style);

        let file_name_len: u16 = file_name
            .len()
            .try_into()
            .expect("A file name length did not fit within a u16");

        for x in name_pos_x + file_name_len..=rect.right_x() {
            screen.set_cell_style(x, y, ' ', draw_style);
        }
    }
}

fn draw_first_column(screen: &mut Screen, fm: &mut FileManager) {
    let first_column_rect = Rect {
        left_x: fm.drawing_info.first_left_x,
        top_y: 1,
        width: fm.drawing_info.first_right_x - fm.drawing_info.first_left_x,
        height: fm.drawing_info.column_height,
    };

    if let Some(prev_dir) = &fm.dir_states.prev_dir {
        let result_column_info = find_correct_location(
            &fm.left_paths,
            fm.drawing_info.column_height,
            prev_dir,
            &fm.dir_states.prev_entries,
            &fm.dir_states.current_dir,
        );

        let starting_index = result_column_info.starting_index;
        let entry_index = result_column_info.starting_index + result_column_info.display_offset;

        draw_column(
            screen,
            first_column_rect,
            starting_index,
            entry_index,
            &fm.dir_states.prev_entries,
        );
    }
}

fn insert_executable<'a>(
    available_execs: &mut HashMap<&'a str, std::path::PathBuf>,
    executable_name: &'a str,
) {
    match which(executable_name) {
        Ok(path) => {
            available_execs.insert(executable_name, path);
        }
        Err(which::Error::CannotFindBinaryPath) => (), // Do nothing when binary not found
        Err(err) => {
            panic!("{}", err);
        }
    }
}

fn find_match_positions(current_entries: &[DirEntryInfo], search_term: &str) -> Vec<usize> {
    current_entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry_info)| {
            if entry_info
                .dir_entry
                .file_name()
                .to_str()
                .unwrap()
                .to_lowercase()
                .contains(&search_term.to_lowercase())
            {
                Some(index)
            } else {
                None
            }
        })
        .collect()
}

fn set_current_dir<P: AsRef<Path>>(
    new_current_dir: P,
    dir_states: &mut DirStates,
    match_positions: &mut Vec<usize>,
) -> crossterm::Result<()> {
    dir_states.set_current_dir(new_current_dir)?;
    match_positions.clear();

    Ok(())
}

fn enter_entry(fm: &mut FileManager, second_entry_index: u16) -> crossterm::Result<()> {
    // NOTE(Chris): We only need to abort asynchronous "image" drawing if we're opening a
    // directory¸ since we're now drawing directory previews asychronously with the same system as
    // the image drawing.

    if fm.dir_states.current_entries.len() <= 0 {
        return Ok(());
    }

    save_location(fm, second_entry_index);

    let selected_entry_path = &fm.dir_states.current_entries[second_entry_index as usize]
        .dir_entry
        .path();

    // TODO(Chris): Show this error without crashing the program
    let selected_target_file_type = match selected_entry_path.metadata() {
        Ok(metadata) => metadata.file_type(),
        Err(_) => return Ok(()),
    };

    if selected_target_file_type.is_dir() {
        abort_image_handles(&mut fm.image_handles);

        let selected_dir_path = selected_entry_path;

        match set_current_dir(
            selected_dir_path,
            &mut fm.dir_states,
            &mut fm.match_positions,
        ) {
            Ok(_) => (),
            Err(err) => match err.kind() {
                io::ErrorKind::PermissionDenied => {
                    // TODO(Chris): Implement an error message for permission being denied
                    return Ok(());
                }
                _ => panic!("{}", err),
            },
        }

        match fm.left_paths.get(selected_dir_path) {
            Some(dir_location) => {
                let curr_entry_index = fm
                    .dir_states
                    .current_entries
                    .iter()
                    .position(|entry| entry.dir_entry.path() == *dir_location.dir_path);

                match curr_entry_index {
                    Some(curr_entry_index) => {
                        let orig_entry_index =
                            (dir_location.starting_index + dir_location.display_offset) as usize;
                        if curr_entry_index == orig_entry_index {
                            fm.second.starting_index = dir_location.starting_index;
                            fm.second.display_offset = dir_location.display_offset;
                        } else {
                            fm.second.starting_index = (curr_entry_index / 2) as u16;
                            fm.second.display_offset =
                                (curr_entry_index as u16) - fm.second.starting_index;
                        }
                    }
                    None => {
                        fm.second.starting_index = 0;
                        fm.second.display_offset = 0;
                    }
                }
            }
            None => {
                fm.second.starting_index = 0;
                fm.second.display_offset = 0;
            }
        };
    } else if selected_target_file_type.is_file() {
        if cfg!(windows) {
            open::that(selected_entry_path)?;
        } else {
            // Should we display some sort of error message according to the exit status
            // here?
            open::that_in_background(selected_entry_path);
        }
    }

    Ok(())
}

// Sets the values underlying column_starting_index and column_display_offset to properly set a
// cursor at the next_position index in a vector of entries.
fn find_column_pos(
    current_entries_len: usize,
    column_height: u16,
    column: ColumnInfo,
    next_position: usize,
) -> crossterm::Result<ColumnInfo> {
    assert!(next_position <= current_entries_len);

    let second_entry_index = column.starting_index + column.display_offset;

    // let lower_offset = (column.height * 2 / 3) as usize;
    // let upper_offset = (column.height / 3) as usize;
    let lesser_offset = SCROLL_OFFSET as usize;
    let greater_offset = (column_height - SCROLL_OFFSET - 1) as usize;

    let mut result_column = column;

    if column_height as usize > current_entries_len {
        assert_eq!(column.starting_index, 0);

        result_column.display_offset = next_position as u16;
    } else if next_position < second_entry_index as usize {
        // Moving up
        if next_position <= lesser_offset {
            result_column.starting_index = 0;

            result_column.display_offset = next_position as u16;
        } else if next_position <= result_column.starting_index as usize + lesser_offset {
            result_column.display_offset = lesser_offset as u16;

            result_column.starting_index = next_position as u16 - result_column.display_offset;
        } else if next_position > result_column.starting_index as usize + lesser_offset {
            result_column.display_offset = next_position as u16 - result_column.starting_index;
        }
    } else if next_position > second_entry_index as usize {
        // Moving down
        if next_position <= result_column.starting_index as usize + greater_offset {
            result_column.display_offset = next_position as u16 - result_column.starting_index;
        } else if next_position > result_column.starting_index as usize + greater_offset {
            result_column.display_offset = greater_offset as u16;

            result_column.starting_index = next_position as u16 - result_column.display_offset;
        } else {
            panic!();
        }

        // Stop us from going too far down the third column
        if result_column.starting_index > current_entries_len as u16 - column_height {
            result_column.starting_index = current_entries_len as u16 - column_height;

            result_column.display_offset = next_position as u16 - result_column.starting_index;
        }
    } else if next_position == second_entry_index as usize {
        // Do nothing.
    } else {
        panic!();
    }

    assert_eq!(
        next_position,
        (result_column.starting_index + result_column.display_offset) as usize
    );

    Ok(result_column)
}

fn update_drawing_info_from_resize(drawing_info: &mut DrawingInfo) -> crossterm::Result<()> {
    let (width, height) = terminal::size()?;
    // Represents the bottom-most y-cell of a column
    let column_bot_y = height - 2;
    // Represents the number of cells in a column vertically.
    let column_height = height - 2;

    *drawing_info = DrawingInfo {
        win_pixels: os_abstract::get_win_pixels()?,
        width,
        height,
        column_bot_y,
        column_height,
        first_left_x: 0,
        first_right_x: width / 6 - 2,
        second_left_x: width / 6,
        second_right_x: width / 2 - 2,
        third_left_x: width / 2,
        third_right_x: width - 2,
    };

    Ok(())
}

// Handle for a task which displays an image
struct DrawHandle {
    can_draw: Arc<AtomicBool>,
}

fn preview_image_or_video(
    win_pixels: WindowPixels,
    third_file: PathBuf,
    ext: String,
    width: u16,
    height: u16,
    left_x: u16,
    image_protocol: ImageProtocol,
) -> io::Result<ImageBufferRgba> {
    let win_px_width = win_pixels.width;
    let win_px_height = win_pixels.height;

    let mut img = match ext.as_str() {
        "mp4" | "webm" | "mkv" => {
            let input = third_file.to_str().unwrap();

            let ffprobe_output = Command::new("ffprobe")
                .args(&[
                    "-loglevel",
                    "error",
                    "-of",
                    "csv=p=0",
                    "-show_entries",
                    "format=duration",
                    input,
                ])
                .output()
                .unwrap();

            let ffprobe_stdout = std::str::from_utf8(&ffprobe_output.stdout).unwrap().trim();

            // Truncate the decimal portion
            let video_duration = ffprobe_stdout.parse::<f64>().unwrap() as i64;

            let ffmpeg_output = Command::new("ffmpeg")
                .args(&[
                    "-ss",
                    &format!("{}", video_duration / 2),
                    "-i",
                    input,
                    "-frames:v",
                    "1",
                    "-c:v",
                    "ppm",
                    "-f",
                    "image2pipe",
                    "pipe:1",
                ])
                .output()
                .unwrap();

            let decoder = image::pnm::PnmDecoder::new(&ffmpeg_output.stdout[..]).unwrap();
            image::DynamicImage::from_decoder(decoder).unwrap()
        }
        // TODO(Chris): Look into using libjpeg-turbo (https://github.com/ImageOptim/mozjpeg-rust)
        // to decode large jpegs faster
        _ => image::io::Reader::open(&third_file)?.decode().unwrap(),
    };

    // NOTE(Chris): sxiv only rotates jpgs somewhat-correctly, but Eye of
    // Gnome (eog) rotates them correctly

    // Rotate jpgs according to their orientation value
    // One-iteration loop for early break
    loop {
        if ext == "jpg" || ext == "jpeg" {
            let bytes = std::fs::read(&third_file)?;

            // Find the location of the Exif header
            let exif_header = b"Exif\x00\x00";
            let exif_header_index = match tiff::find_bytes(&bytes, exif_header) {
                Some(value) => value,
                None => break,
            };

            // This assumes that the beginning of the TIFF section
            // comes right after the Exif header
            let tiff_index = exif_header_index + exif_header.len();
            let tiff_bytes = &bytes[tiff_index..];

            let byte_order = match &tiff_bytes[0..=1] {
                b"II" => Endian::LittleEndian,
                b"MM" => Endian::BigEndian,
                _ => panic!("Unable to determine endianness of TIFF section!"),
            };

            if tiff_bytes[2] != 42 && tiff_bytes[3] != 42 {
                panic!("Could not confirm existence of TIFF section with 42!");
            }

            // From the beginning of the TIFF section
            let first_ifd_offset = usizeify(&tiff_bytes[4..=7], byte_order);

            let num_ifd_entries = usizeify(
                &tiff_bytes[first_ifd_offset..first_ifd_offset + 2],
                byte_order,
            );

            let first_ifd_entry_offset = first_ifd_offset + 2;

            // NOTE(Chris): We don't actually need info on all of the
            // IFD entries, but I'm too lazy to break early from the
            // for loop
            let mut ifd_entries = vec![];
            for entry_index in 0..num_ifd_entries {
                let entry_bytes = &tiff_bytes[first_ifd_entry_offset + (12 * entry_index)..];
                let entry = IFDEntry::from_slice(entry_bytes, byte_order);
                ifd_entries.push(entry);
            }

            let orientation_ifd = ifd_entries.iter().find(|entry| {
                entry.tag == EntryTag::Orientation
                    && entry.field_type == EntryType::Short
                    && entry.count == 1
            });

            let orientation_value = match orientation_ifd {
                Some(value) => value,
                None => break,
            };

            match orientation_value.value_offset {
                1 => (),
                2 => img = img.fliph(),
                3 => img = img.rotate180(),
                4 => img = img.flipv(),
                5 => img = img.rotate90().fliph(),
                6 => img = img.rotate90(),
                7 => img = img.rotate270().fliph(),
                8 => img = img.rotate270(),
                _ => (),
            }

            tiff::IFDEntry::from_slice(&bytes, byte_order);
        }

        break;
    }

    let (img_width, img_height) = img.dimensions();

    let mut img_cells_width = img_width * (width as u32) / (win_px_width as u32);
    let mut img_cells_height = img_height * (height as u32) / (win_px_height as u32);

    // eprintln!("   image: {:?}", &third_file);

    // eprintln!(
    //     "beginning - img_cells_width: {:3}, img_cells_height: {:3}",
    //     img_cells_width, img_cells_height
    // );

    let orig_img_cells_width = img_cells_width;
    let orig_img_cells_height = img_cells_height;

    // let third_column_width = width - left_x - 2;

    let third_column_width = (width - left_x - 2) as u32;
    // Subtract 1 because columns start at y = 1, subtract 1 again
    // because columns stop at the penultimate row
    let third_column_height = (height - 2) as u32;

    // eprintln!(
    //     "               column_width: {:3},    column_height: {:3}",
    //     third_column_width, third_column_height
    // );

    // Scale the image down to fit the width, if necessary
    if img_cells_width > third_column_width {
        img_cells_width = third_column_width;
    }

    // Scale the image even further down to fit the height, if
    // necessary
    if img_cells_height > third_column_height {
        img_cells_height = third_column_height;
    }

    // eprintln!(
    //     "   ending - img_cells_width: {:3}, img_cells_height: {:3}",
    //     img_cells_width, img_cells_height
    // );

    if orig_img_cells_width != img_cells_width || orig_img_cells_height != img_cells_height {
        let display_width_px = img_cells_width * (win_px_width as u32) / (width as u32);
        let display_height_px = img_cells_height * (win_px_height as u32) / (height as u32);

        if orig_img_cells_width > third_column_width * 3
            || orig_img_cells_height > third_column_height * 3
        {
            img = img.thumbnail(display_width_px, display_height_px);
        } else {
            img = img.resize(
                display_width_px,
                display_height_px,
                image::imageops::FilterType::Triangle,
            );
        }
    }

    let rgba = img.to_rgba8();

    Ok(rgba)
}

fn draw_bottom_info_line(screen: &mut Screen, fm: &mut FileManager) {
    // TODO(Chris): Display info for empty directory when in empty directory, like in lf
    if fm.dir_states.current_entries.len() <= 0 {
        return;
    }

    let updated_second_entry_index = fm.second.starting_index + fm.second.display_offset;

    let extra_perms = os_abstract::get_extra_perms(
        &fm.dir_states.current_entries[updated_second_entry_index as usize].metadata,
    );

    let mode_str = &extra_perms.mode;

    let mut draw_style = Style::new_attr(rolf_grid::Attribute::Bold);

    let mut info_line_builder = LineBuilder::new();

    let colored_mode = {
        let mut colored_mode = vec![];
        // The Windows mode string is only 6 characters long, so this avoids the Windows mode
        // string.
        if mode_str.len() > 6 {
            // TODO(Chris): Reimplement this on Windows
            // queue!(colored_mode, style::SetAttribute(Attribute::Bold)).unwrap();
        }
        for (index, byte) in mode_str.bytes().enumerate() {
            if index > 3 {
                draw_style.attribute = rolf_grid::Attribute::None;
            }

            match &[byte] {
                b"d" => {
                    draw_style.fg = rolf_grid::Color::Blue;
                    info_line_builder.push(byte as char, draw_style);
                }
                b"r" => {
                    draw_style.fg = rolf_grid::Color::Yellow;
                    info_line_builder.push(byte as char, draw_style);
                }
                b"w" => {
                    draw_style.fg = rolf_grid::Color::Red;
                    info_line_builder.push(byte as char, draw_style);
                }
                b"x" => {
                    draw_style.fg = rolf_grid::Color::Green;
                    info_line_builder.push(byte as char, draw_style);
                }
                b"-" => {
                    draw_style.fg = rolf_grid::Color::Blue;
                    info_line_builder.push(byte as char, draw_style);
                }
                b"l" => {
                    draw_style.attribute = rolf_grid::Attribute::None;
                    draw_style.fg = rolf_grid::Color::Cyan;

                    info_line_builder.push(byte as char, draw_style);

                    draw_style.attribute = rolf_grid::Attribute::Bold;
                    draw_style.fg = rolf_grid::Color::Foreground;
                }
                b"c" | b"b" => {
                    queue!(colored_mode, style::SetForegroundColor(Color::DarkYellow),).unwrap();
                    colored_mode.push(byte);
                }
                _ => {
                    queue!(colored_mode, style::SetForegroundColor(Color::Reset),).unwrap();
                    colored_mode.push(byte);
                }
            }
        }

        colored_mode
    };

    // TODO(Chris): Display user/group names in white if they are not the current user/the current
    // user is not in the group

    if let Some(hard_link_count) = extra_perms.hard_link_count {
        info_line_builder
            .use_fg_color(rolf_grid::Color::Red)
            .use_attribute(rolf_grid::Attribute::Bold)
            .push_str(&format!(" {:2}", hard_link_count));
    }

    if let Some(user_name) = extra_perms.user_name {
        info_line_builder
            .use_fg_color(rolf_grid::Color::Yellow)
            .use_attribute(rolf_grid::Attribute::Bold)
            .push_str(&format!(" {:2}", user_name));
    }

    if let Some(group_name) = extra_perms.group_name {
        info_line_builder
            .use_fg_color(rolf_grid::Color::Yellow)
            .use_attribute(rolf_grid::Attribute::Bold)
            .push_str(&format!(" {}", group_name));
    }

    if let Some(size) = extra_perms.size {
        info_line_builder
            .use_fg_color(rolf_grid::Color::Green)
            .use_attribute(rolf_grid::Attribute::Bold)
            .push_str(&format!(" {:>4}", human_size(size)));
    }

    if let Some(modify_date_time) = extra_perms.modify_date_time {
        info_line_builder
            .use_fg_color(rolf_grid::Color::Blue)
            .use_attribute(rolf_grid::Attribute::None)
            .push_str(" ")
            .push_str(&modify_date_time);
    }

    let display_position = format!(
        "{}/{}",
        updated_second_entry_index + 1,
        fm.dir_states.current_entries.len()
    );

    screen.build_line(0, fm.drawing_info.height - 1, &info_line_builder);

    draw_str(
        screen,
        fm.drawing_info.width - (display_position.len() as u16),
        fm.drawing_info.height - 1,
        &display_position,
        rolf_grid::Style::default(),
    );
}

fn abort_image_handles(image_handles: &mut Vec<DrawHandle>) {
    while !image_handles.is_empty() {
        let image_handle = image_handles.pop().unwrap();
        image_handle
            .can_draw
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

fn store_in_tmp_file(buf: &[u8]) -> std::result::Result<std::path::PathBuf, io::Error> {
    let (mut tmpfile, path) = tempfile::Builder::new()
        .prefix(".tmp.rolf")
        .rand_bytes(1)
        .tempfile()?
        // Since the file is persisted, the user is responsible for deleting it afterwards. However,
        // Kitty does this automatically after printing from a temp file.
        .keep()?;

    tmpfile.write_all(buf)?;
    tmpfile.flush()?;
    Ok(path)
}

fn format_current_dir(dir_states: &DirStates, home_path: &Path) -> String {
    // NOTE(Chris): This creates a new String, and it'd be nice to avoid making a heap
    // allocation here, but it's probably not worth trying to figure out how to use only a str

    if dir_states.current_dir == *home_path {
        String::from("~")
    } else if dir_states.current_dir.starts_with(home_path) {
        // "~"
        format!(
            "~{}{}",
            path::MAIN_SEPARATOR,
            dir_states
                .current_dir
                .strip_prefix(home_path)
                .unwrap()
                .to_str()
                .unwrap()
        )
    } else if dir_states.prev_dir.is_none() {
        String::from("")
    } else {
        dir_states.current_dir.to_str().unwrap().to_string()
    }
}

// For the list consisting of the entries in parent_entries, find the correct display offset and
// starting index that will put the cursor on dir
fn find_correct_location(
    left_paths: &HashMap<std::path::PathBuf, DirLocation>,
    column_height: u16,
    parent_dir: &Path,
    parent_entries: &[DirEntryInfo],
    dir: &Path,
) -> ColumnInfo {
    return match left_paths.get(parent_dir) {
        Some(dir_location) => ColumnInfo {
            display_offset: dir_location.display_offset,
            starting_index: dir_location.starting_index,
        },
        None => {
            let first_bottom_index = column_height;

            let parent_entry_index = parent_entries
                .iter()
                .position(|entry| entry.dir_entry.path() == *dir)
                .unwrap();

            if parent_entry_index < first_bottom_index as usize {
                ColumnInfo {
                    starting_index: 0,
                    display_offset: parent_entry_index as u16,
                }
            } else {
                let entries_len = parent_dir.read_dir().unwrap().count();

                find_column_pos(
                    entries_len,
                    column_height,
                    // NOTE(Chris): It's not clear that we'd want to use a less-hacky ColumnInfo
                    ColumnInfo {
                        starting_index: 0,
                        display_offset: 0,
                    },
                    parent_entry_index,
                )
                .unwrap()
            }
        }
    };
}

#[derive(Debug)]
struct DirLocation {
    dir_path: std::path::PathBuf,
    starting_index: u16,
    display_offset: u16,
}

#[derive(Debug)]
struct DirStates {
    current_dir: std::path::PathBuf,
    current_entries: Vec<DirEntryInfo>,
    prev_dir: Option<std::path::PathBuf>,
    prev_entries: Vec<DirEntryInfo>,
}

impl DirStates {
    fn new() -> crossterm::Result<DirStates> {
        // This is a slightly wasteful way to do this, but I'm too lazy to add anything better
        let mut dir_states = DirStates {
            current_dir: PathBuf::with_capacity(0),
            current_entries: Vec::with_capacity(0),
            prev_dir: None,
            prev_entries: Vec::with_capacity(0),
        };

        dir_states.set_current_dir(std::env::current_dir().unwrap())?;

        Ok(dir_states)
    }

    fn set_current_dir<P: AsRef<Path>>(self: &mut DirStates, path: P) -> crossterm::Result<()> {
        std::env::set_current_dir(&path)?;

        self.current_dir = path.as_ref().to_path_buf();

        self.current_entries = get_sorted_entries(&self.current_dir).unwrap();

        let parent_path = self.current_dir.parent();
        match parent_path {
            Some(parent_path) => {
                let parent_path = parent_path.to_path_buf();
                self.prev_entries = get_sorted_entries(&parent_path).unwrap();
                self.prev_dir = Some(parent_path);
            }
            None => {
                self.prev_entries = vec![];
                self.prev_dir = None;
            }
        };

        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RecordedFileType {
    File,
    Directory,
    FileSymlink,
    DirectorySymlink,
    InvalidSymlink,
    Other,
}

#[derive(Debug)]
struct DirEntryInfo {
    dir_entry: DirEntry,
    metadata: Metadata,
    file_type: RecordedFileType,
}

enum BroadFileType {
    File,
    Directory,
}

fn broaden_file_type(file_type: &RecordedFileType) -> BroadFileType {
    match file_type {
        RecordedFileType::File
        | RecordedFileType::FileSymlink
        | RecordedFileType::InvalidSymlink
        | RecordedFileType::Other => BroadFileType::File,
        RecordedFileType::Directory | RecordedFileType::DirectorySymlink => {
            BroadFileType::Directory
        }
    }
}

// Sorts std::fs::DirEntry by file type first (with directory coming before files),
// then by file name. Symlinks are ignored in favor of the original files' file types.
// lf seems to do this with symlinks as well.
// TODO(Chris): Get rid of all the zany unwrap() calls in this function, since it's not supposed to
// fail
fn cmp_dir_entry_info(entry_info_1: &DirEntryInfo, entry_info_2: &DirEntryInfo) -> Ordering {
    let broad_ft_1 = broaden_file_type(&entry_info_1.file_type);
    let broad_ft_2 = broaden_file_type(&entry_info_2.file_type);

    match (broad_ft_1, broad_ft_2) {
        (BroadFileType::Directory, BroadFileType::File) => Ordering::Less,
        (BroadFileType::File, BroadFileType::Directory) => Ordering::Greater,
        _ => cmp_natural(
            entry_info_1.dir_entry.file_name().to_str().unwrap(),
            entry_info_2.dir_entry.file_name().to_str().unwrap(),
        ),
    }
}

fn save_location(fm: &mut FileManager, second_entry_index: u16) {
    fm.left_paths.insert(
        fm.dir_states.current_dir.clone(),
        DirLocation {
            dir_path: fm.dir_states.current_entries[second_entry_index as usize]
                .dir_entry
                .path(),
            starting_index: fm.second.starting_index,
            display_offset: fm.second.display_offset,
        },
    );
}

type ImageBufferRgba = ImageBuffer<Rgba<u8>, Vec<u8>>;

enum PreviewData {
    Loading,
    Blank,
    Directory { entries_info: Vec<DirEntryInfo> },
    UncoloredFile { path: PathBuf },
    ImageBuffer { buffer: ImageBufferRgba },
    RawBytes { bytes: Vec<u8> },
}

#[derive(Clone, Copy)]
struct Rect {
    left_x: u16,
    top_y: u16,
    width: u16,
    height: u16,
}

impl Rect {
    // If a Rect conceptually has a top_y of 1 and a bot_y of 2, it will have a height of 1.
    fn bot_y(&self) -> u16 {
        self.top_y + self.height
    }

    fn right_x(&self) -> u16 {
        self.left_x + self.width
    }
}

fn draw_str(screen: &mut Screen, x: u16, y: u16, string: &str, style: Style) {
    for (i, ch) in string.char_indices() {
        let i: u16 = i.try_into().expect("Should be able to fit into a u16.");
        screen.set_cell_style(x + i, y, ch, style);
    }
}

fn buf_entries_column(
    screen: &mut Screen,
    rect: Rect,
    entries: &[DirEntryInfo],
    column: ColumnInfo,
    selections: &SelectionsMap,
) -> io::Result<()> {
    if entries.len() <= 0 {
        draw_str(
            screen,
            rect.left_x + 1,
            rect.top_y,
            "empty",
            Style::new_attr(rolf_grid::Attribute::Reverse),
        );
    } else {
        for y in rect.top_y..=rect.top_y + rect.height {
            let ind: usize = (y - rect.top_y).into();

            if ind > entries.len() {
                break;
            }

            let file_info = &entries[ind];

            draw_str(
                screen,
                rect.left_x,
                y,
                file_info.dir_entry.file_name().to_str().unwrap(),
                Style::default(),
            );
        }
    }

    Ok(())
}

fn get_sorted_entries<P: AsRef<Path>>(path: P) -> io::Result<Vec<DirEntryInfo>> {
    let mut entries = std::fs::read_dir(path)?
        .filter_map(|entry| {
            let dir_entry = entry.unwrap();
            let entry_path = dir_entry.path();
            let metadata = match std::fs::symlink_metadata(&entry_path) {
                Ok(metadata) => metadata,
                // TODO(Chris): Handles error in this case in more detail
                Err(_) => return None,
            };

            let file_type = {
                let curr_file_type = metadata.file_type();

                if curr_file_type.is_file() {
                    RecordedFileType::File
                } else if curr_file_type.is_dir() {
                    RecordedFileType::Directory
                } else if curr_file_type.is_symlink() {
                    match fs::canonicalize(&entry_path) {
                        Ok(canonical_path) => {
                            let canonical_metadata = fs::metadata(canonical_path).unwrap();
                            let canonical_file_type = canonical_metadata.file_type();

                            if canonical_file_type.is_file() {
                                RecordedFileType::FileSymlink
                            } else if canonical_file_type.is_dir() {
                                RecordedFileType::DirectorySymlink
                            } else {
                                RecordedFileType::Other
                            }
                        }
                        Err(err) => match err.kind() {
                            io::ErrorKind::NotFound => RecordedFileType::InvalidSymlink,
                            _ => Err(err).unwrap(),
                        },
                    }
                } else {
                    RecordedFileType::Other
                }
            };

            Some(DirEntryInfo {
                dir_entry,
                metadata,
                file_type,
            })
        })
        .collect::<Vec<DirEntryInfo>>();

    entries.sort_by(cmp_dir_entry_info);

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_column_pos_1() {
        let result_column = find_column_pos(
            53,
            28,
            // NOTE(Chris): It's not clear that we'd want to use a less-hacky ColumnInfo
            ColumnInfo {
                starting_index: 0,
                display_offset: 0,
            },
            38,
        )
        .unwrap();

        assert_eq!(
            result_column,
            ColumnInfo {
                starting_index: 21,
                display_offset: 17,
            }
        );
    }

    #[test]
    fn test_find_column_pos_2() {
        let result_column = find_column_pos(
            130,
            28,
            // NOTE(Chris): It's not clear that we'd want to use a less-hacky ColumnInfo
            ColumnInfo {
                starting_index: 0,
                display_offset: 0,
            },
            81,
        )
        .unwrap();

        assert_eq!(
            result_column,
            ColumnInfo {
                starting_index: 64,
                display_offset: 17,
            }
        );
    }
}
