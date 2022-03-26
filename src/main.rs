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
use std::ffi::{OsStr, OsString};
use std::fs::{self, DirEntry, Metadata};
use std::io::{self, BufRead, BufWriter, StdoutLock, Write};
use std::path::{self, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::vec::Vec;

use image::{ColorType, GenericImageView, ImageEncoder};

use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinHandle;

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute, queue,
    style::{self, Attribute, Color, Stylize},
    terminal::{self, ClearType},
};

use rolf_grid::Style;
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
    };

    update_drawing_info_from_resize(&mut fm.drawing_info)?;

    let screen = Screen::new(io::stdout())?;
    let screen = Mutex::new(screen);

    let mut command_queue = config_ast.clone();

    // Main input loop
    'input: loop {
        let second_entry_index = fm.second.starting_index + fm.second.display_offset;

        let second_bottom_index = fm.second.starting_index + fm.drawing_info.column_height;

        // TODO(Chris): Use the unicode-segmentation package to count graphemes
        // Add 1 because of the ':' that is displayed after user_host_display
        // Add 1 again because of the '/' that is displayed at the end of current_dir_display
        // let remaining_width = fm.drawing_info.width as usize
        //     - (fm.user_host_display.len() + 1 + current_dir_display.len() + 1);

        // let file_stem = if file_stem.len() > remaining_width {
        //     String::from(&file_stem[..remaining_width])
        // } else {
        //     String::from(file_stem)
        // };

        match fm.input_mode {
            InputMode::Normal => {
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
                                    // FIXME(Chris): Reimplement this
                                    // enter_entry(&mut screen_lock, &mut fm, second_entry_index)?;
                                }
                                // NOTE(Chris): lf doesn't actually provide a specific command for this, instead using
                                // a default keybinding that takes advantage of EDITOR
                                "edit" => {
                                    let editor = match std::env::var("VISUAL") {
                                        Err(std::env::VarError::NotPresent) => {
                                            match std::env::var("EDITOR") {
                                                Err(std::env::VarError::NotPresent) => {
                                                    String::from("")
                                                }
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
                                        let selected_entry = &fm.dir_states.current_entries
                                            [second_entry_index as usize];

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

                                        let old_starting_index = fm.second.starting_index;
                                        let old_display_offset = fm.second.display_offset;

                                        fm.second.starting_index = 0;
                                        fm.second.display_offset = 0;
                                    }
                                }
                                "bottom" => {
                                    if !fm.dir_states.current_entries.is_empty() {
                                        abort_image_handles(&mut fm.image_handles);

                                        let old_starting_index = fm.second.starting_index;
                                        let old_display_offset = fm.second.display_offset;

                                        if fm.dir_states.current_entries.len()
                                            <= (fm.drawing_info.column_height as usize)
                                        {
                                            fm.second.starting_index = 0;
                                            fm.second.display_offset =
                                                fm.dir_states.current_entries.len() as u16 - 1;
                                        } else {
                                            fm.second.display_offset =
                                                fm.drawing_info.column_height - 1;
                                            fm.second.starting_index =
                                                fm.dir_states.current_entries.len() as u16
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

                {
                    // NOTE(Chris): Recompute second_entry_index since the relevant values may have
                    // been modified
                    let second_entry_index = fm.second.starting_index + fm.second.display_offset;

                    let mut screen_lock = screen.lock().expect("Failed to lock screen mutex!");
                    let screen_lock = &mut *screen_lock;

                    screen_lock.clear_logical();

                    draw_str(screen_lock, 0, 0, "Hello!", rolf_grid::Style::default());

                    draw_str(screen_lock, 0, 1, "There!", rolf_grid::Style::default());

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

                    draw_column(
                        screen_lock,
                        third_column_rect,
                        0,
                        0,
                        &fm.dir_states.current_entries,
                    );

                    screen_lock.show()?;
                }

                let event = event::read()?;

                match event {
                    Event::Key(event) => {
                        if let Some(bound_command) = fm.config.keybindings.get(&event) {
                            // FIXME(Chris): Handle the possible error here
                            command_queue.push(parse_statement_from(bound_command).unwrap());
                        }
                    }
                    Event::Mouse(_) => (),
                    Event::Resize(_, _) => {}
                }
            }
            InputMode::Command => {
                // let line_from_user = get_cmd_line_input(w, ":", &mut fm)?;

                // // If there was no input line returned, then the user aborted the use of the
                // // command line. Thus, we only need to do anything when an input line is actually
                // // returned.
                // if let Some(line_from_user) = line_from_user {
                //     let trimmed_input_line = line_from_user.trim();
                //     let spaced_words: Vec<&str> = trimmed_input_line.split_whitespace().collect();

                //     if !spaced_words.is_empty() {
                //         match spaced_words[0] {
                //             "search" => {
                //                 if spaced_words.len() == 2 {
                //                     let search_term = spaced_words[1];

                //                     fm.match_positions = find_match_positions(
                //                         &fm.dir_states.current_entries,
                //                         search_term,
                //                     );

                //                     fm.should_search_forwards = true;
                //                 }
                //             }
                //             "search-back" => {
                //                 if spaced_words.len() == 2 {
                //                     let search_term = spaced_words[1];

                //                     fm.match_positions = find_match_positions(
                //                         &fm.dir_states.current_entries,
                //                         search_term,
                //                     );

                //                     fm.should_search_forwards = false;
                //                 }
                //             }
                //             "rename" => {
                //                 // Get the full path of the current file
                //                 let current_file = &fm.dir_states.current_entries
                //                     [second_entry_index as usize]
                //                     .dir_entry;
                //                 let current_file_path = current_file.path();

                //                 // TODO(Chris): Get rid of these unwrap calls (at least the OsStr
                //                 // to str conversion one)
                //                 fm.input_line.push_str(
                //                     current_file_path.file_name().unwrap().to_str().unwrap(),
                //                 );

                //                 let new_name = get_cmd_line_input(w, "Rename: ", &mut fm)?;

                //                 if let Some(new_name) = new_name {
                //                     let new_file_path = current_file_path
                //                         .parent()
                //                         .unwrap()
                //                         .join(PathBuf::from(&new_name));
                //                     fs::rename(current_file_path, new_file_path)?;

                //                     set_current_dir(
                //                         fm.dir_states.current_dir.clone(),
                //                         &mut fm.dir_states,
                //                         &mut fm.match_positions,
                //                     )?;

                //                     fm.match_positions = find_match_positions(
                //                         &fm.dir_states.current_entries,
                //                         &new_name,
                //                     );
                //                 }
                //             }
                //             _ => {
                //                 let mut stdout_lock = w.lock();

                //                 queue!(
                //                     stdout_lock,
                //                     terminal::Clear(ClearType::CurrentLine),
                //                     cursor::Hide,
                //                     cursor::MoveToColumn(0),
                //                     style::SetForegroundColor(Color::Grey),
                //                     style::SetBackgroundColor(Color::DarkRed),
                //                     style::Print(format!("invalid command: {}", spaced_words[0])),
                //                     style::SetForegroundColor(Color::Reset),
                //                     style::SetBackgroundColor(Color::Reset),
                //                 )?;
                //             }
                //         }
                //     }
                // }
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

    input_mode: InputMode,

    user_host_display: String,

    selections: SelectionsMap,

    drawing_info: DrawingInfo,

    config: Config,
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
            inner_left_x,
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

        for x in name_pos_x + file_name_len..rect.right_x() {
            screen.set_cell_style(x, y, ' ', draw_style);
        }
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

fn get_cmd_line_input(
    w: &mut io::Stdout,
    prompt: &str,
    fm: &mut FileManager,
) -> io::Result<Option<String>> {
    let mut cursor_index = fm.input_line.len(); // Where a new character will next be entered

    {
        let mut stdout_lock = w.lock();

        execute!(
            &mut stdout_lock,
            cursor::Show,
            style::SetAttribute(Attribute::Reset)
        )?;
    }

    // Command line input loop
    loop {
        // Use this scope when displaying the input prompt and current line
        {
            let mut stdout_lock = w.lock();

            if prompt.is_empty() {
                queue!(
                    &mut stdout_lock,
                    cursor::MoveTo(0, fm.drawing_info.height - 1),
                    terminal::Clear(ClearType::CurrentLine),
                    style::Print(format!(":{}", fm.input_line)),
                    cursor::MoveTo((1 + cursor_index) as u16, fm.drawing_info.height - 1),
                )?;
            } else {
                queue!(
                    &mut stdout_lock,
                    cursor::MoveTo(0, fm.drawing_info.height - 1),
                    terminal::Clear(ClearType::CurrentLine),
                    style::Print(format!("{}{}", prompt, fm.input_line)),
                    cursor::MoveTo(
                        (prompt.len() + cursor_index) as u16,
                        fm.drawing_info.height - 1
                    ),
                )?;
            }

            stdout_lock.flush()?;
        }

        let event = event::read()?;

        match event {
            Event::Key(event) => match event.code {
                KeyCode::Char(ch) => {
                    if event.modifiers.contains(KeyModifiers::CONTROL) {
                        match ch {
                            'b' => {
                                if cursor_index > 0 {
                                    cursor_index -= 1;
                                }
                            }
                            'f' => {
                                if cursor_index < fm.input_line.len() {
                                    cursor_index += 1;
                                }
                            }
                            'a' => cursor_index = 0,
                            'e' => cursor_index = fm.input_line.len(),
                            'c' => {}
                            'k' => {
                                fm.input_line = fm.input_line.chars().take(cursor_index).collect();
                            }
                            _ => (),
                        }
                    } else if event.modifiers.contains(KeyModifiers::ALT) {
                        match ch {
                            'b' => {
                                cursor_index =
                                    line_edit::find_prev_word_pos(&fm.input_line, cursor_index);
                            }
                            'f' => {
                                cursor_index =
                                    line_edit::find_next_word_pos(&fm.input_line, cursor_index);
                            }
                            'd' => {
                                let ending_index =
                                    line_edit::find_next_word_pos(&fm.input_line, cursor_index);
                                fm.input_line.replace_range(cursor_index..ending_index, "");
                            }
                            _ => (),
                        }
                    } else {
                        fm.input_line.insert(cursor_index, ch);

                        cursor_index += 1;
                    }
                }
                KeyCode::Enter => {}
                KeyCode::Left => {
                    if cursor_index > 0 {
                        cursor_index -= 1;
                    }
                }
                KeyCode::Right => {
                    if cursor_index < fm.input_line.len() {
                        cursor_index += 1;
                    }
                }
                KeyCode::Backspace => {
                    if cursor_index > 0 {
                        if event.modifiers.contains(KeyModifiers::ALT) {
                            let ending_index = cursor_index;
                            cursor_index =
                                line_edit::find_prev_word_pos(&fm.input_line, cursor_index);
                            fm.input_line.replace_range(cursor_index..ending_index, "");
                        } else {
                            fm.input_line.remove(cursor_index - 1);

                            cursor_index -= 1;
                        }
                    }
                }
                KeyCode::Esc => {}
                _ => (),
            },
            Event::Mouse(_) => (),
            Event::Resize(_, _) => {}
        }

        assert!(cursor_index <= fm.input_line.len());
    }
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

fn enter_entry(
    stdout_lock: &mut StdoutLock,
    fm: &mut FileManager,
    second_entry_index: u16,
) -> crossterm::Result<()> {
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
    handle: JoinHandle<crossterm::Result<()>>,
    can_draw: Arc<AtomicBool>,
}

// This macro should be used to run asynchronous functions that draw to the screen (specifically,
// in the third column).
// The first parameter to the async function referred to be $async_fn_name should be of the type
// Arc<Mutex<bool>>. All of the arguments to the async function _except for this first one_ should
// be passed in at the end of the macro invocation.
macro_rules! spawn_async_draw {
    ($runtime:expr, $handles:expr, $async_fn_name:expr, $($async_other_args:tt)*) => {
        let can_draw = Arc::new(AtomicBool::new(true));
        let clone = Arc::clone(&can_draw);

        let preview_image_handle = $runtime.spawn($async_fn_name(
                clone,
                $($async_other_args)*
        ));

        $handles.push(DrawHandle {
            handle: preview_image_handle,
            can_draw,
        });
    }
}

async fn preview_uncolored_file(
    can_draw_preview: Arc<AtomicBool>,
    drawing_info: DrawingInfo,
    third_file: PathBuf,
    left_x: u16,
    right_x: u16,
) -> io::Result<()> {
    let can_display_image = can_draw_preview.load(std::sync::atomic::Ordering::Acquire);

    let inner_left_x = left_x + 2;

    if can_display_image {
        let file = fs::File::open(third_file)?;
        let stdout = io::stdout();
        let mut w = stdout.lock();

        let mut curr_y = 1; // Columns start at y = 1

        queue!(
            &mut w,
            style::SetAttribute(Attribute::Reset),
            terminal::DisableLineWrap
        )?;

        // Clear the first line, in case there's a Loading... message already there
        queue!(&mut w, cursor::MoveTo(inner_left_x, 1))?;
        for _curr_x in inner_left_x..=right_x {
            queue!(&mut w, style::Print(' '))?;
        }

        let max_line_length = (right_x - inner_left_x) as usize;

        for line in io::BufReader::new(file)
            .lines()
            .take(drawing_info.column_height as usize)
            .flatten()
        {
            queue!(&mut w, cursor::MoveTo(inner_left_x, curr_y))?;

            if line.len() > max_line_length {
                writeln!(&mut w, "{}", &line[0..=max_line_length])?;
            } else {
                writeln!(&mut w, "{}", line)?;
            }

            curr_y += 1;
        }

        // Clear the right-most edge of the terminal, since it might
        // have been drawn over when printing file contents
        for curr_y in 1..=drawing_info.column_bot_y {
            queue!(
                &mut w,
                cursor::MoveTo(drawing_info.width, curr_y),
                style::Print(' ')
            )?;
        }

        queue!(&mut w, terminal::EnableLineWrap)?;

        w.flush()?;
    }

    Ok(())
}

async fn preview_image_or_video(
    can_display_image: Arc<AtomicBool>,
    win_pixels: WindowPixels,
    third_file: PathBuf,
    ext: String,
    width: u16,
    height: u16,
    left_x: u16,
    image_protocol: ImageProtocol,
) -> crossterm::Result<()> {
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

    let stdout = io::stdout();

    let rgba = img.to_rgba8();

    match image_protocol {
        ImageProtocol::Kitty => {
            let raw_img = rgba.as_raw();

            let mut w = stdout.lock();
            let can_display_image = can_display_image.load(std::sync::atomic::Ordering::Acquire);

            if can_display_image {
                let path = store_in_tmp_file(raw_img)?;

                queue!(
                    w,
                    cursor::MoveTo(left_x, 1),
                    // Hide the "Should display!" / "Loading..." message
                    style::Print("               "),
                    cursor::MoveTo(left_x, 1),
                )?;

                write!(
                    w,
                    "\x1b_Gf=32,s={},v={},a=T,t=t;{}\x1b\\",
                    img.width(),
                    img.height(),
                    base64::encode(path.to_str().unwrap())
                )?;

                w.flush()?;
            }
        }
        ImageProtocol::ITerm2 => {
            let mut png_data = vec![];
            {
                let mut writer = BufWriter::new(&mut png_data);
                PngEncoder::new(&mut writer)
                    .write_image(&rgba, rgba.width(), rgba.height(), ColorType::Rgba8)
                    .unwrap();
            }

            let mut w = stdout.lock();
            let can_display_image = can_display_image.load(std::sync::atomic::Ordering::Acquire);

            if can_display_image {
                if cfg!(windows) {
                    queue!(w, cursor::MoveTo(left_x, 1), style::Print("  "),)?;
                } else {
                    // By adding 2, we match the location of lf's Loading...
                    let inner_left_x = left_x + 2;

                    queue!(
                        w,
                        cursor::MoveTo(inner_left_x, 1),
                        style::Print("          "),
                        cursor::MoveTo(left_x, 1),
                    )?;
                }

                write!(
                    w,
                    "\x1b]1337;File=size={};inline=1:{}\x1b\\",
                    png_data.len(),
                    base64::encode(png_data),
                )?;

                w.flush()?;
            }
        }
        _ => (),
    }

    Ok(())
}

async fn preview_source_file(
    can_display_image: Arc<AtomicBool>,
    drawing_info: DrawingInfo,
    third_file: PathBuf,
    left_x: u16,
    right_x: u16,
    highlight: PathBuf,
) -> crossterm::Result<()> {
    let inner_left_x = left_x + 2;

    // TODO(Chris): Actually show that something went wrong
    let output = Command::new(highlight)
        .arg("-O")
        .arg("ansi")
        .arg("--max-size=500K")
        .arg(&third_file)
        .output()
        .unwrap();

    let can_display_image = can_display_image.load(std::sync::atomic::Ordering::Acquire);

    if can_display_image {
        // NOTE(Chris): Since we're locking can_display_image above and stdout here, we should be
        // wary of deadlock
        let stdout = io::stdout();
        let mut w = stdout.lock();

        // Clear the first line, in case there's a Loading... message already there
        queue!(&mut w, cursor::MoveTo(inner_left_x, 1))?;
        for _curr_x in inner_left_x..=right_x {
            queue!(&mut w, style::Print(' '))?;
        }

        // TODO(Chris): Handle case when file is not valid utf8
        if let Ok(text) = std::str::from_utf8(&output.stdout) {
            let mut curr_y = 1; // Columns start at y = 1
            queue!(&mut w, cursor::MoveTo(inner_left_x, curr_y))?;

            queue!(&mut w, terminal::DisableLineWrap)?;

            for ch in text.as_bytes() {
                if curr_y > drawing_info.column_bot_y {
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

            // TODO(Chris): Figure out why the right-most edge of the
            // terminal sometimes has a character that should be one beyond
            // that right-most edge. This bug occurs when right-most edge
            // isn't blanked out (as is currently done below).

            // Clear the right-most edge of the terminal, since it might
            // have been drawn over when printing file contents
            for curr_y in 1..=drawing_info.column_bot_y {
                queue!(
                    &mut w,
                    cursor::MoveTo(drawing_info.width, curr_y),
                    style::Print(' ')
                )?;
            }
        }

        w.flush()?;
    }

    Ok(())
}

fn abort_image_handles(image_handles: &mut Vec<DrawHandle>) {
    while !image_handles.is_empty() {
        let image_handle = image_handles.pop().unwrap();
        image_handle
            .can_draw
            .store(false, std::sync::atomic::Ordering::Release);
        image_handle.handle.abort();
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

// Queues a third column with a single highlighted line
fn queue_oneline_column(
    w: &mut StdoutLock,
    left_x: u16,
    right_x: u16,
    column_bot_y: u16,
    message: &str,
) -> crossterm::Result<()> {
    let mut curr_y = 1; // 1 is the starting y for columns
    let col_width = right_x - left_x + 1;

    queue!(
        w,
        cursor::MoveTo(left_x, curr_y),
        style::SetAttribute(Attribute::Reverse),
        style::SetForegroundColor(Color::White),
        style::Print(message),
        style::SetAttribute(Attribute::NoReverse),
    )?;
    queue!(w, cursor::MoveTo(left_x + (message.len() as u16), curr_y))?;
    for _ in message.len()..(col_width as usize) {
        queue!(w, style::Print(' '))?;
    }

    curr_y += 1;

    // NOTE(Chris): This loop is redundant when this function is used to draw in the third column,
    // Ensure that the bottom of "short buffers" are properly cleared
    while curr_y <= column_bot_y {
        queue!(w, cursor::MoveTo(left_x, curr_y))?;

        for _ in 0..col_width {
            queue!(w, style::Print(' '))?;
        }

        curr_y += 1;
    }

    Ok(())
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

#[derive(Debug)]
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

fn update_entries_column(
    w: &mut io::StdoutLock,
    fm: &mut FileManager,
    old_offset: u16,
    old_start_index: u16,
) -> crossterm::Result<()> {
    let left_x = fm.drawing_info.second_left_x;
    let right_x = fm.drawing_info.second_right_x;
    let column_bot_y = fm.drawing_info.column_bot_y;
    let new = fm.second;

    if new.starting_index != old_start_index {
        queue_entries_column(
            w,
            left_x,
            right_x,
            column_bot_y,
            &fm.dir_states.current_entries,
            new,
            &fm.selections,
        )?;
        return Ok(());
    }

    queue!(w, style::SetAttribute(Attribute::Reset))?;

    // Update the old offset
    queue_full_entry(
        w,
        &fm.dir_states.current_entries,
        left_x,
        right_x,
        old_offset,
        old_start_index,
        &fm.selections,
        false,
    )?;

    // Update the new offset
    queue_full_entry(
        w,
        &fm.dir_states.current_entries,
        left_x,
        right_x,
        new.display_offset,
        new.starting_index,
        &fm.selections,
        true,
    )?;

    Ok(())
}

// NOTE(Chris): This draws outside of the left_x -> right_x line, drawing markers of selection to
// at left_x - 1.
fn queue_full_entry(
    w: &mut io::StdoutLock,
    entries: &[DirEntryInfo],
    left_x: u16,
    right_x: u16,
    display_offset: u16,
    starting_index: u16,
    selections: &SelectionsMap,
    highlighted: bool,
) -> crossterm::Result<()> {
    let new_entry_index = starting_index + display_offset;
    let new_entry_info = &entries[new_entry_index as usize];
    let new_file_type = new_entry_info.metadata.file_type();

    if new_file_type.is_dir() {
        queue!(
            w,
            style::SetForegroundColor(Color::DarkBlue),
            style::SetAttribute(Attribute::Bold),
        )?;
    } else if new_file_type.is_file() {
        queue!(w, style::SetForegroundColor(Color::White))?;
    } else if new_file_type.is_symlink() {
        let color = match std::fs::metadata(new_entry_info.dir_entry.path()) {
            Ok(_) => Color::DarkCyan,
            // This assumes that if there is an error, it is because the symlink points to an
            // invalid target
            Err(_) => Color::DarkRed,
        };

        queue!(
            w,
            style::SetForegroundColor(color),
            style::SetAttribute(Attribute::Bold)
        )?;
    }

    let w: &mut io::StdoutLock = w;
    let left_x = left_x;
    let right_x = right_x;
    let display_offset = display_offset;
    let file_name = new_entry_info.dir_entry.file_name();
    let file_name = file_name.to_str().unwrap();
    let inner_left_x = left_x + 1;
    let mut curr_x = inner_left_x; // This is the cell which we are about to print into.

    if selections.contains_key(&new_entry_info.dir_entry.path()) {
        queue!(
            w,
            cursor::MoveTo(left_x, display_offset + 1),
            style::SetBackgroundColor(Color::DarkMagenta),
            style::Print(' '),
            style::SetBackgroundColor(Color::Reset),
        )?;
    } else {
        queue!(
            w,
            cursor::MoveTo(left_x, display_offset + 1),
            style::SetBackgroundColor(Color::Reset),
            style::Print(' ')
        )?;
    }

    if highlighted {
        queue!(w, style::SetAttribute(Attribute::Reverse))?;
    }

    queue!(
        w,
        cursor::MoveTo(curr_x, display_offset + 1),
        style::Print(' ')
    )?; // 1 is the starting y for columns
    curr_x += 1;

    // NOTE(Chris): In lf, we start by printing an initial space. This is already done above.
    // If the file name is smaller than the column width - 2, print the name and then add spaces
    // until the end of the column
    // If the file name is exactly the column width - 2, print the name and then add spaces until
    // the end of the column (which is now just one space)
    // If the file name is more than the column width - 2, print the name until the end of column -
    // 2, then add a "~" (this is in column - 1),
    // then add spaces until the end of the column (which is now just one space)

    // NOTE(Chris): "until" here means up to and including that cell
    // The "end of column" is the last cell in the column
    // A column does not include the gaps in between columns (there's an uncolored gap on the side
    // of each column, resulting in there being a two-cell gap between any two columns)

    // This is the number of cells in the column. If right_x and left_x were equal, there would
    // still be exactly one cell in the column, which is why we add 1.
    let col_width = (right_x - left_x + 1) as usize;

    let file_name_len = file_name.chars().count();

    if file_name_len <= col_width - 2 {
        queue!(w, style::Print(file_name))?;
        // This conversion is fine since file_name.len() can't be longer than
        // the terminal width in this instance.
        curr_x += file_name.len() as u16;
    } else {
        // Print the name until the end of column - 2
        for ch in file_name.chars() {
            // If curr_x == right_x - 1, then a character was printed into right_x - 2 in the
            // previous iteration of the loop
            if curr_x == right_x - 1 {
                break;
            }

            queue!(w, style::Print(ch))?;

            curr_x += 1;
        }

        assert!(curr_x == right_x - 1);

        // This '~' is now in column - 1
        queue!(w, style::Print('~'))?;
        curr_x += 1;
    }

    while curr_x <= right_x {
        queue!(w, style::Print(' '))?;

        curr_x += 1;
    }

    if new_file_type.is_dir() || new_file_type.is_symlink() {
        queue!(w, style::SetAttribute(Attribute::NormalIntensity))?;
    }

    if highlighted {
        queue!(w, style::SetAttribute(Attribute::NoReverse))?;
    }

    Ok(())
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

fn queue_entries_column(
    w: &mut io::StdoutLock,
    left_x: u16,
    right_x: u16,
    bottom_y: u16,
    entries: &[DirEntryInfo],
    column: ColumnInfo,
    selections: &SelectionsMap,
) -> crossterm::Result<()> {
    let mut curr_y = 1; // 1 is the starting y for columns

    queue!(w, style::SetAttribute(Attribute::Reset))?;
    if entries.len() <= 0 {
        queue!(
            w,
            cursor::MoveTo(left_x, curr_y),
            style::Print("  "),
            style::SetAttribute(Attribute::Reverse),
            style::SetForegroundColor(Color::White),
            style::Print("empty"),
            style::SetAttribute(Attribute::Reset),
            style::Print(" "),
        )?;

        let mut curr_x = left_x + 8; // Length of "  empty "

        while curr_x <= right_x {
            queue!(w, style::Print(' '))?;

            curr_x += 1;
        }

        curr_y += 1;
    } else {
        let our_entries = &entries[column.starting_index as usize..];
        for _entry in our_entries {
            if curr_y > bottom_y {
                break;
            }

            let is_curr_entry = curr_y - 1 == column.display_offset;

            queue_full_entry(
                w,
                entries,
                left_x,
                right_x,
                curr_y - 1,
                column.starting_index,
                selections,
                is_curr_entry,
            )?;

            curr_y += 1;
        }
    }

    let col_width = right_x - left_x + 1;

    // NOTE(Chris): This loop is redundant when this function is used to draw in the third column,
    // since that column is cleared in preparation for asynchronous drawing.
    // Ensure that the bottom of "short buffers" are properly cleared
    while curr_y <= bottom_y {
        queue!(w, cursor::MoveTo(left_x, curr_y))?;

        for _ in 0..=col_width {
            queue!(w, style::Print(' '))?;
        }

        curr_y += 1;
    }

    Ok(())
}

fn queue_blank_column(
    w: &mut StdoutLock,
    left_x: u16,
    right_x: u16,
    column_height: u16,
) -> crossterm::Result<()> {
    let mut curr_y = 1; // 1 is the starting y for columns

    while curr_y <= column_height {
        queue!(w, cursor::MoveTo(left_x, curr_y))?;

        let mut curr_x = left_x;
        while curr_x <= right_x {
            queue!(w, style::Print(' '))?;

            curr_x += 1;
        }

        curr_y += 1;
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
