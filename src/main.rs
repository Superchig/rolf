mod natural_sort; // This declares the existence of the natural_sort module, which searches by
                  // default for natural_sort.rs or natural_sort/mod.rs

mod human_size;
mod line_edit;
mod strmode;
mod tiff;
mod unix_users;

use human_size::human_size;
use natural_sort::cmp_natural;
use tiff::{usizeify, Endian, EntryTag, EntryType, IFDEntry};

use open;
use strmode::strmode;
use which::which;

use std::cmp::Ordering;
use std::collections::hash_map::HashMap;
use std::fs::{DirEntry, Metadata};
use std::io::{self, StdoutLock, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::vec::Vec;

use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;

use image::GenericImageView;

use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinHandle;

use chrono::offset::TimeZone;
use chrono::prelude::{DateTime, Local, NaiveDateTime};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute, queue,
    style::{self, Attribute, Color, Stylize},
    terminal::{self, ClearType},
};

// TODO(Chris): Make this configurable rather than hard-coding the constant
const SCROLL_OFFSET: u16 = 10;

type HandlesVec = Vec<ImageHandle>;

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

    terminal::enable_raw_mode()?;

    queue!(
        w,
        terminal::EnterAlternateScreen,
        style::ResetColor,
        terminal::Clear(ClearType::All),
        cursor::Hide,
    )?;

    let result = run(&mut w);

    execute!(
        w,
        style::ResetColor,
        cursor::Show,
        terminal::LeaveAlternateScreen,
    )?;

    terminal::disable_raw_mode()?;

    match result {
        Ok(current_dir) => match last_dir_path {
            Some(last_dir_path) => {
                std::fs::write(last_dir_path, current_dir.to_str().unwrap()).unwrap()
            }
            None => (),
        },
        Err(err) => panic!("{}", err),
    }

    Ok(())
}

// Returns the path to the last dir
fn run(w: &mut io::Stdout) -> crossterm::Result<PathBuf> {
    let user_name = match std::env::var("USER") {
        Ok(val) => val,
        Err(e) => panic!("Could not read $USER environment variable: {}", e),
    };

    let host_name = get_hostname().unwrap();

    let home_name = match std::env::var("HOME") {
        Ok(val) => val,
        Err(e) => panic!("Could not read $HOME: {}", e),
    };

    let mut available_execs: HashMap<&str, std::path::PathBuf> = HashMap::new();

    available_execs.insert("highlight", which("highlight").unwrap());

    available_execs.insert("ffmpeg", which("ffmpeg").unwrap());

    let home_path = Path::new(&home_name[..]);

    // NOTE(Chris): The default column ratio is 1:2:3

    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();

    let mut image_handles = vec![];

    let mut dir_states = DirStates::new()?;

    let mut second_display_offset = 0;

    let mut is_first_iteration = true;

    let mut second_starting_index = 0;

    let mut left_paths: HashMap<std::path::PathBuf, DirLocation> = HashMap::new();

    let mut win_pixels = get_win_pixels()?;

    let mut should_enter_cmd_line = false;

    let mut match_positions: Vec<usize> = vec![];

    let mut should_search_forwards = true;

    let mut input_line = String::new();

    let user_host_display = format!("{}@{}", user_name, host_name);

    // Main input loop
    loop {
        // Gather all the data before rendering things with stdout_lock

        // The terminal's height is also the index of the lowest cell
        let (width, height) = terminal::size()?;
        let (second_column, column_bot_y, column_height) = calc_second_column_info(width, height);

        let second_bottom_index = second_starting_index + column_height;

        let current_dir_display = format_current_dir(&dir_states, home_path);

        let second_entry_index = second_starting_index + second_display_offset;

        let curr_entry;
        let file_stem = if dir_states.current_entries.len() <= 0 {
            ""
        } else {
            curr_entry = dir_states.current_entries[second_entry_index as usize]
                .dir_entry
                .file_name();
            curr_entry.to_str().unwrap()
        };

        // TODO(Chris): Use the unicode-segmentation package to count graphemes
        let remaining_width =
            width as usize - (user_host_display.len() + 1 + current_dir_display.len() + 1);

        // Add 1 because of the ':' that is displayed after user_host_display
        // Add 1 again because of the '/' that is displayed at the end of current_dir_display
        let file_stem = if file_stem.len() > remaining_width {
            // format!("{}~", &file_stem[..remaining_width - 2])
            String::from(&file_stem[..remaining_width])
        } else {
            String::from(file_stem)
        };

        // TODO(Chris): Check if we're currently using the kitty terminal (or anything which
        // supports its image protocol)

        {
            let mut stdout_lock = w.lock();

            queue!(
                stdout_lock,
                cursor::MoveTo(0, 0),
                terminal::Clear(ClearType::CurrentLine),
                style::SetForegroundColor(Color::DarkGreen),
                style::SetAttribute(Attribute::Bold),
                style::Print(&user_host_display),
                style::SetForegroundColor(Color::White),
                style::Print(":"),
                style::SetForegroundColor(Color::DarkBlue),
                style::Print(format!("{}/", current_dir_display)),
                style::SetForegroundColor(Color::White),
                style::Print(file_stem),
            )?;

            if is_first_iteration {
                queue_all_columns(
                    &mut stdout_lock,
                    &runtime,
                    &mut image_handles,
                    &win_pixels,
                    &dir_states,
                    &left_paths,
                    &available_execs,
                    width,
                    height,
                    column_height,
                    column_bot_y,
                    second_column,
                    second_display_offset,
                    second_starting_index,
                )?;

                is_first_iteration = false;
            }

            stdout_lock.flush()?;
        }

        {
            let event = event::read()?;

            let mut stdout_lock = w.lock();

            match event {
                Event::Key(event) => match event.code {
                    KeyCode::Char(ch) => {
                        match ch {
                            'q' => break,
                            'h' => {
                                abort_image_handles(&mut image_handles);

                                let old_current_dir = dir_states.current_dir.clone();
                                if dir_states.current_entries.len() > 0 {
                                    save_location(
                                        &mut left_paths,
                                        &dir_states,
                                        second_entry_index,
                                        second_starting_index,
                                        second_display_offset,
                                    );
                                }

                                if let Some(parent_dir) = dir_states.prev_dir.clone() {
                                    set_current_dir(
                                        parent_dir,
                                        &mut dir_states,
                                        &mut match_positions,
                                    )?;
                                }

                                let (display_offset, starting_index) = find_correct_location(
                                    &left_paths,
                                    column_height,
                                    &dir_states.current_dir,
                                    &dir_states.current_entries,
                                    &old_current_dir,
                                );
                                second_display_offset = display_offset;
                                second_starting_index = starting_index;

                                stdout_lock.write(b"\x1b_Ga=d;\x1b\\")?; // Delete all visible images

                                queue_all_columns(
                                    &mut stdout_lock,
                                    &runtime,
                                    &mut image_handles,
                                    &win_pixels,
                                    &dir_states,
                                    &left_paths,
                                    &available_execs,
                                    width,
                                    height,
                                    column_height,
                                    column_bot_y,
                                    second_column,
                                    second_display_offset,
                                    second_starting_index,
                                )?;
                            }
                            'l' => {
                                enter_entry(
                                    &mut stdout_lock,
                                    &runtime,
                                    &mut image_handles,
                                    &available_execs,
                                    &mut dir_states,
                                    &mut match_positions,
                                    &mut left_paths,
                                    win_pixels,
                                    width,
                                    height,
                                    second_entry_index,
                                    &mut second_starting_index,
                                    &mut second_display_offset,
                                    column_height,
                                    column_bot_y,
                                    second_column,
                                )?;
                            }
                            'j' => {
                                if dir_states.current_entries.len() > 0
                                    && (second_entry_index as usize)
                                        < dir_states.current_entries.len() - 1
                                {
                                    abort_image_handles(&mut image_handles);

                                    let old_starting_index = second_starting_index;
                                    let old_display_offset = second_display_offset;

                                    if second_display_offset >= (column_height - SCROLL_OFFSET - 1)
                                        && (second_bottom_index as usize)
                                            < dir_states.current_entries.len()
                                    {
                                        second_starting_index += 1;
                                    } else if second_entry_index < second_bottom_index {
                                        second_display_offset += 1;
                                    }

                                    queue_entry_changed(
                                        &mut stdout_lock,
                                        &runtime,
                                        &mut image_handles,
                                        &win_pixels,
                                        &dir_states,
                                        &left_paths,
                                        &available_execs,
                                        width,
                                        height,
                                        column_height,
                                        column_bot_y,
                                        old_starting_index,
                                        old_display_offset,
                                        second_starting_index,
                                        second_display_offset,
                                        second_column,
                                    )?;
                                }
                            }
                            'k' => {
                                if dir_states.current_entries.len() > 0 {
                                    abort_image_handles(&mut image_handles);

                                    let old_starting_index = second_starting_index;
                                    let old_display_offset = second_display_offset;

                                    if second_display_offset <= (SCROLL_OFFSET)
                                        && second_starting_index > 0
                                    {
                                        second_starting_index -= 1;
                                    } else if second_entry_index > 0 {
                                        second_display_offset -= 1;
                                    }

                                    queue_entry_changed(
                                        &mut stdout_lock,
                                        &runtime,
                                        &mut image_handles,
                                        &win_pixels,
                                        &dir_states,
                                        &left_paths,
                                        &available_execs,
                                        width,
                                        height,
                                        column_height,
                                        column_bot_y,
                                        old_starting_index,
                                        old_display_offset,
                                        second_starting_index,
                                        second_display_offset,
                                        second_column,
                                    )?;
                                }
                            }
                            'e' => {
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
                                if editor != "" {
                                    let selected_entry =
                                        &dir_states.current_entries[second_entry_index as usize];

                                    let shell_command = format!(
                                        "{} {}",
                                        editor,
                                        selected_entry
                                            .dir_entry
                                            .path()
                                            .to_str()
                                            .expect("Failed to convert path to string")
                                    );

                                    queue!(stdout_lock, terminal::LeaveAlternateScreen)?;

                                    Command::new("sh")
                                        .arg("-c")
                                        .arg(shell_command)
                                        .status()
                                        .expect("failed to execute editor command");

                                    queue!(
                                        stdout_lock,
                                        terminal::EnterAlternateScreen,
                                        cursor::Hide
                                    )?;

                                    queue_all_columns(
                                        &mut stdout_lock,
                                        &runtime,
                                        &mut image_handles,
                                        &win_pixels,
                                        &dir_states,
                                        &left_paths,
                                        &available_execs,
                                        width,
                                        height,
                                        column_height,
                                        column_bot_y,
                                        second_column,
                                        second_display_offset,
                                        second_starting_index,
                                    )?;
                                }
                            }
                            'g' => {
                                if dir_states.current_entries.len() > 0 {
                                    abort_image_handles(&mut image_handles);

                                    let old_starting_index = second_starting_index;
                                    let old_display_offset = second_display_offset;

                                    second_starting_index = 0;
                                    second_display_offset = 0;

                                    update_entries_column(
                                        &mut stdout_lock,
                                        second_column,
                                        width / 2 - 2,
                                        column_bot_y,
                                        &dir_states.current_entries,
                                        old_display_offset,
                                        old_starting_index,
                                        second_display_offset,
                                        second_starting_index,
                                    )?;

                                    queue_third_column(
                                        &mut stdout_lock,
                                        &runtime,
                                        &mut image_handles,
                                        &win_pixels,
                                        &dir_states,
                                        &left_paths,
                                        &available_execs,
                                        width,
                                        height,
                                        column_height,
                                        column_bot_y,
                                        (second_starting_index + second_display_offset) as usize,
                                    )?;

                                    queue_bottom_info_line(
                                        &mut stdout_lock,
                                        width,
                                        height,
                                        second_starting_index,
                                        second_display_offset,
                                        &dir_states,
                                    )?;
                                }
                            }
                            'G' => {
                                if dir_states.current_entries.len() > 0 {
                                    abort_image_handles(&mut image_handles);

                                    let old_starting_index = second_starting_index;
                                    let old_display_offset = second_display_offset;

                                    if dir_states.current_entries.len() <= (column_height as usize)
                                    {
                                        second_starting_index = 0;
                                        second_display_offset =
                                            dir_states.current_entries.len() as u16 - 1;
                                    } else {
                                        second_display_offset = column_height - 1;
                                        second_starting_index = dir_states.current_entries.len()
                                            as u16
                                            - second_display_offset
                                            - 1;
                                    }

                                    update_entries_column(
                                        &mut stdout_lock,
                                        second_column,
                                        width / 2 - 2,
                                        column_bot_y,
                                        &dir_states.current_entries,
                                        old_display_offset,
                                        old_starting_index,
                                        second_display_offset,
                                        second_starting_index,
                                    )?;

                                    queue_third_column(
                                        &mut stdout_lock,
                                        &runtime,
                                        &mut image_handles,
                                        &win_pixels,
                                        &dir_states,
                                        &left_paths,
                                        &available_execs,
                                        width,
                                        height,
                                        column_height,
                                        column_bot_y,
                                        (second_starting_index + second_display_offset) as usize,
                                    )?;

                                    queue_bottom_info_line(
                                        &mut stdout_lock,
                                        width,
                                        height,
                                        second_starting_index,
                                        second_display_offset,
                                        &dir_states,
                                    )?;
                                }
                            }
                            ':' => {
                                should_enter_cmd_line = true;
                            }
                            '/' => {
                                assert!(input_line.len() <= 0);

                                input_line.push_str("search ");

                                should_enter_cmd_line = true;
                            }
                            '?' => {
                                assert!(input_line.len() <= 0);

                                input_line.push_str("search-back ");

                                should_enter_cmd_line = true;
                            }
                            'n' => {
                                queue_search_jump(
                                    &mut stdout_lock,
                                    &match_positions,
                                    &runtime,
                                    &mut image_handles,
                                    &win_pixels,
                                    &dir_states,
                                    &left_paths,
                                    &available_execs,
                                    should_search_forwards,
                                    width,
                                    height,
                                    column_height,
                                    column_bot_y,
                                    &mut second_starting_index,
                                    &mut second_display_offset,
                                    second_column,
                                )?;
                            }
                            'N' => {
                                queue_search_jump(
                                    &mut stdout_lock,
                                    &match_positions,
                                    &runtime,
                                    &mut image_handles,
                                    &win_pixels,
                                    &dir_states,
                                    &left_paths,
                                    &available_execs,
                                    !should_search_forwards,
                                    width,
                                    height,
                                    column_height,
                                    column_bot_y,
                                    &mut second_starting_index,
                                    &mut second_display_offset,
                                    second_column,
                                )?;
                            }
                            _ => (),
                        }
                    }
                    KeyCode::Enter => enter_entry(
                        &mut stdout_lock,
                        &runtime,
                        &mut image_handles,
                        &available_execs,
                        &mut dir_states,
                        &mut match_positions,
                        &mut left_paths,
                        win_pixels,
                        width,
                        height,
                        second_entry_index,
                        &mut second_starting_index,
                        &mut second_display_offset,
                        column_height,
                        column_bot_y,
                        second_column,
                    )?,
                    _ => (),
                },
                Event::Mouse(_) => (),
                Event::Resize(_, _) => {
                    redraw_upper(
                        &mut stdout_lock,
                        &mut win_pixels,
                        &runtime,
                        &mut image_handles,
                        &dir_states,
                        &left_paths,
                        &available_execs,
                        second_starting_index,
                        second_display_offset,
                    )?;

                    let (width, height) = terminal::size()?;

                    queue_bottom_info_line(
                        &mut stdout_lock,
                        width,
                        height,
                        second_starting_index,
                        second_display_offset,
                        &dir_states,
                    )?;
                }
            }
        }

        // The command line code is activated via a flag so that we aren't stuck with stdout
        // locked (which would happen if we put this code in the input-handling match statements
        // above)
        if should_enter_cmd_line {
            should_enter_cmd_line = false;

            let mut cursor_index = input_line.len(); // Where a new character will next be entered

            {
                let mut stdout_lock = w.lock();

                queue!(
                    &mut stdout_lock,
                    style::SetAttribute(Attribute::Reset),
                    cursor::Show,
                    cursor::MoveTo(0, height - 1),
                    terminal::Clear(ClearType::CurrentLine),
                    style::Print(':'),
                    cursor::MoveTo(1, height - 1),
                    style::Print(&input_line),
                    cursor::MoveTo(1 + cursor_index as u16, height - 1),
                )?;

                stdout_lock.flush()?;
            }

            // Command line input loop
            loop {
                let event = event::read()?;

                let mut stdout_lock = w.lock();

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
                                        if cursor_index < input_line.len() {
                                            cursor_index += 1;
                                        }
                                    }
                                    'a' => cursor_index = 0,
                                    'e' => cursor_index = input_line.len(),
                                    'c' => {
                                        queue_cmd_line_exit(
                                            &mut &mut stdout_lock,
                                            &dir_states,
                                            width,
                                            height,
                                            second_starting_index,
                                            second_display_offset,
                                            &mut input_line,
                                        )?;

                                        break;
                                    }
                                    'k' => {
                                        input_line =
                                            input_line.chars().take(cursor_index).collect();
                                    }
                                    _ => (),
                                }
                            } else if event.modifiers.contains(KeyModifiers::ALT) {
                                match ch {
                                    'b' => {
                                        cursor_index = line_edit::find_prev_word_pos(
                                            &input_line,
                                            cursor_index,
                                        );
                                    }
                                    'f' => {
                                        cursor_index = line_edit::find_next_word_pos(
                                            &input_line,
                                            cursor_index,
                                        );
                                    }
                                    'd' => {
                                        let ending_index = line_edit::find_next_word_pos(
                                            &input_line,
                                            cursor_index,
                                        );
                                        input_line.replace_range(cursor_index..ending_index, "");
                                    }
                                    _ => (),
                                }
                            } else {
                                input_line.insert(cursor_index, ch);

                                cursor_index += 1;
                            }
                        }
                        KeyCode::Enter => {
                            let trimmed_input_line = input_line.trim();
                            let spaced_words: Vec<&str> =
                                trimmed_input_line.split_whitespace().collect();

                            if spaced_words.len() > 0 {
                                match spaced_words[0] {
                                    "search" => {
                                        if spaced_words.len() == 2 {
                                            let search_term = spaced_words[1];

                                            match_positions = dir_states
                                                .current_entries
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
                                                .collect();

                                            should_search_forwards = true;

                                            queue_search_jump(
                                                &mut stdout_lock,
                                                &match_positions,
                                                &runtime,
                                                &mut image_handles,
                                                &win_pixels,
                                                &dir_states,
                                                &left_paths,
                                                &available_execs,
                                                should_search_forwards,
                                                width,
                                                height,
                                                column_height,
                                                column_bot_y,
                                                &mut second_starting_index,
                                                &mut second_display_offset,
                                                second_column,
                                            )?;
                                        }
                                    }
                                    "search-back" => {
                                        if spaced_words.len() == 2 {
                                            let search_term = spaced_words[1];

                                            match_positions = dir_states
                                                .current_entries
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
                                                .collect();

                                            should_search_forwards = false;

                                            queue_search_jump(
                                                &mut stdout_lock,
                                                &match_positions,
                                                &runtime,
                                                &mut image_handles,
                                                &win_pixels,
                                                &dir_states,
                                                &left_paths,
                                                &available_execs,
                                                should_search_forwards,
                                                width,
                                                height,
                                                column_height,
                                                column_bot_y,
                                                &mut second_starting_index,
                                                &mut second_display_offset,
                                                second_column,
                                            )?;
                                        }
                                    }
                                    _ => (),
                                }

                                queue_cmd_line_exit(
                                    &mut &mut stdout_lock,
                                    &dir_states,
                                    width,
                                    height,
                                    second_starting_index,
                                    second_display_offset,
                                    &mut input_line,
                                )?;

                                break;
                            }
                        }
                        KeyCode::Left => {
                            if cursor_index > 0 {
                                cursor_index -= 1;
                            }
                        }
                        KeyCode::Right => {
                            if cursor_index < input_line.len() {
                                cursor_index += 1;
                            }
                        }
                        KeyCode::Backspace => {
                            if cursor_index > 0 {
                                if event.modifiers.contains(KeyModifiers::ALT) {
                                    let ending_index = cursor_index;
                                    cursor_index =
                                        line_edit::find_prev_word_pos(&input_line, cursor_index);
                                    input_line.replace_range(cursor_index..ending_index, "");
                                } else {
                                    input_line.remove(cursor_index - 1);

                                    cursor_index -= 1;
                                }
                            }
                        }
                        KeyCode::Esc => {
                            queue_cmd_line_exit(
                                &mut &mut stdout_lock,
                                &dir_states,
                                width,
                                height,
                                second_starting_index,
                                second_display_offset,
                                &mut input_line,
                            )?;

                            break;
                        }
                        _ => (),
                    },
                    Event::Mouse(_) => (),
                    Event::Resize(_, _) => {
                        redraw_upper(
                            &mut stdout_lock,
                            &mut win_pixels,
                            &runtime,
                            &mut image_handles,
                            &dir_states,
                            &left_paths,
                            &available_execs,
                            second_starting_index,
                            second_display_offset,
                        )?;
                    }
                }

                assert!(cursor_index <= input_line.len());

                queue!(
                    &mut stdout_lock,
                    cursor::MoveTo(0, height - 1),
                    terminal::Clear(ClearType::CurrentLine),
                    style::Print(format!(":{}", input_line)),
                    cursor::MoveTo((1 + cursor_index) as u16, height - 1),
                )?;

                stdout_lock.flush()?;
            }
        }
    }

    Ok(dir_states.current_dir)
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
    mut stdout_lock: &mut StdoutLock,
    runtime: &Runtime,
    mut image_handles: &mut Vec<ImageHandle>,
    available_execs: &HashMap<&str, std::path::PathBuf>,
    mut dir_states: &mut DirStates,
    mut match_positions: &mut Vec<usize>,
    mut left_paths: &mut HashMap<std::path::PathBuf, DirLocation>,
    win_pixels: WindowPixels,
    width: u16,
    height: u16,
    second_entry_index: u16,
    second_starting_index: &mut u16,
    second_display_offset: &mut u16,
    column_height: u16,
    column_bot_y: u16,
    second_column: u16,
) -> crossterm::Result<()> {
    // NOTE(Chris): We don't need to abort image handles here. If we are entering a
    // directory, then the previous current entry was a directory, and we were never
    // displaying an image. If we are entering a file, then we aren't changing the current
    // file, so there's no need to abort the image display.

    if dir_states.current_entries.len() <= 0 {
        return Ok(());
    }

    save_location(
        &mut left_paths,
        &dir_states,
        second_entry_index,
        *second_starting_index,
        *second_display_offset,
    );

    let selected_entry_path = &dir_states.current_entries[second_entry_index as usize]
        .dir_entry
        .path();

    // TODO(Chris): Show this error without crashing the program
    let selected_target_file_type = match selected_entry_path.metadata() {
        Ok(metadata) => metadata.file_type(),
        Err(_) => return Ok(()),
    };

    if selected_target_file_type.is_dir() {
        let selected_dir_path = selected_entry_path;

        set_current_dir(selected_dir_path, &mut dir_states, &mut match_positions)?;

        match left_paths.get(selected_dir_path) {
            Some(dir_location) => {
                let curr_entry_index = dir_states
                    .current_entries
                    .iter()
                    .position(|entry| entry.dir_entry.path() == *dir_location.dir_path);

                match curr_entry_index {
                    Some(curr_entry_index) => {
                        let orig_entry_index =
                            (dir_location.starting_index + dir_location.display_offset) as usize;
                        if curr_entry_index == orig_entry_index {
                            *second_starting_index = dir_location.starting_index;
                            *second_display_offset = dir_location.display_offset;
                        } else {
                            *second_starting_index = (curr_entry_index / 2) as u16;
                            *second_display_offset =
                                (curr_entry_index as u16) - *second_starting_index;
                        }
                    }
                    None => {
                        *second_starting_index = 0;
                        *second_display_offset = 0;
                    }
                }
            }
            None => {
                *second_starting_index = 0;
                *second_display_offset = 0;
            }
        };

        queue_all_columns(
            &mut stdout_lock,
            &runtime,
            &mut image_handles,
            &win_pixels,
            &dir_states,
            &left_paths,
            &available_execs,
            width,
            height,
            column_height,
            column_bot_y,
            second_column,
            *second_display_offset,
            *second_starting_index,
        )?;
    } else if selected_target_file_type.is_file() {
        // Should we display some sort of error message according to the exit status
        // here?
        open::that_in_background(selected_entry_path);
    }

    Ok(())
}

fn queue_search_jump(
    mut stdout_lock: &mut StdoutLock,
    match_positions: &Vec<usize>,
    runtime: &Runtime,
    mut image_handles: &mut Vec<ImageHandle>,
    win_pixels: &WindowPixels,
    dir_states: &DirStates,
    left_paths: &HashMap<std::path::PathBuf, DirLocation>,
    available_execs: &HashMap<&str, std::path::PathBuf>,
    should_search_forwards: bool,
    width: u16,
    height: u16,
    column_height: u16,
    column_bot_y: u16,
    second_starting_index: &mut u16,
    second_display_offset: &mut u16,
    second_column: u16,
) -> crossterm::Result<()> {
    if match_positions.len() <= 0 {
        return Ok(());
    }

    let second_entry_index = *second_starting_index + *second_display_offset;

    let next_position = if should_search_forwards {
        let result = match_positions
            .iter()
            .find(|pos| **pos > second_entry_index as usize);

        match result {
            None => match_positions[0],
            Some(next_position) => *next_position,
        }
    } else {
        let result = match_positions
            .iter()
            .rev()
            .find(|pos| **pos < second_entry_index as usize);

        match result {
            None => *match_positions.last().unwrap(),
            Some(next_position) => *next_position,
        }
    };

    let old_starting_index = *second_starting_index;
    let old_display_offset = *second_display_offset;

    // let lower_offset = (column_height * 2 / 3) as usize;
    // let upper_offset = (column_height / 3) as usize;
    let lesser_offset = SCROLL_OFFSET as usize;
    let greater_offset = (column_height - SCROLL_OFFSET - 1) as usize;

    if column_height as usize > dir_states.current_entries.len() {
        *second_display_offset = next_position as u16;
    } else if next_position < second_entry_index as usize {
        // Moving up
        if next_position <= lesser_offset {
            *second_starting_index = 0;

            *second_display_offset = next_position as u16;
        } else if next_position <= *second_starting_index as usize + lesser_offset {
            *second_display_offset = lesser_offset as u16;

            *second_starting_index = next_position as u16 - *second_display_offset;
        } else if next_position > *second_starting_index as usize + lesser_offset {
            *second_display_offset = next_position as u16 - *second_starting_index;
        }
    } else if next_position > second_entry_index as usize {
        // Moving down

        // TODO(Chris): See if this first branch can be removed safely
        if next_position <= greater_offset {
            *second_starting_index = 0;

            *second_display_offset = next_position as u16;
        } else if next_position <= *second_starting_index as usize + greater_offset {
            *second_display_offset = next_position as u16 - *second_starting_index;
        } else if next_position > *second_starting_index as usize + greater_offset {
            *second_display_offset = greater_offset as u16;

            *second_starting_index = next_position as u16 - *second_display_offset;
        } else {
            panic!();
        }

        // Stop us from going too far down the third column
        if *second_starting_index > dir_states.current_entries.len() as u16 - column_height {
            *second_starting_index = dir_states.current_entries.len() as u16 - column_height;

            *second_display_offset = next_position as u16 - *second_starting_index;
        }
    } else if next_position == second_entry_index as usize {
        // Do nothing.
    } else {
        panic!();
    }

    assert_eq!(
        next_position,
        (*second_starting_index + *second_display_offset) as usize
    );

    queue_entry_changed(
        &mut stdout_lock,
        &runtime,
        &mut image_handles,
        &win_pixels,
        &dir_states,
        &left_paths,
        &available_execs,
        width,
        height,
        column_height,
        column_bot_y,
        old_starting_index,
        old_display_offset,
        *second_starting_index,
        *second_display_offset,
        second_column,
    )?;

    Ok(())
}

fn queue_entry_changed(
    mut stdout_lock: &mut StdoutLock,
    runtime: &Runtime,
    mut image_handles: &mut Vec<ImageHandle>,
    win_pixels: &WindowPixels,
    dir_states: &DirStates,
    left_paths: &HashMap<std::path::PathBuf, DirLocation>,
    available_execs: &HashMap<&str, std::path::PathBuf>,
    width: u16,
    height: u16,
    column_height: u16,
    column_bot_y: u16,
    old_starting_index: u16,
    old_display_offset: u16,
    second_starting_index: u16,
    second_display_offset: u16,
    second_column: u16,
) -> crossterm::Result<()> {
    update_entries_column(
        &mut stdout_lock,
        second_column,
        width / 2 - 2,
        column_bot_y,
        &dir_states.current_entries,
        old_display_offset,
        old_starting_index,
        second_display_offset,
        second_starting_index,
    )?;

    queue_third_column(
        &mut stdout_lock,
        &runtime,
        &mut image_handles,
        &win_pixels,
        &dir_states,
        &left_paths,
        &available_execs,
        width,
        height,
        column_height,
        column_bot_y,
        (second_starting_index + second_display_offset) as usize,
    )?;

    // NOTE(Chris): We flush here, so the current function is more than a "queue_" function
    stdout_lock.flush()?;

    queue_bottom_info_line(
        &mut stdout_lock,
        width,
        height,
        second_starting_index,
        second_display_offset,
        &dir_states,
    )?;

    Ok(())
}

fn queue_cmd_line_exit(
    mut stdout_lock: &mut StdoutLock,
    dir_states: &DirStates,
    width: u16,
    height: u16,
    second_starting_index: u16,
    second_display_offset: u16,
    input_line: &mut String,
) -> crossterm::Result<()> {
    input_line.clear();

    queue!(
        stdout_lock,
        terminal::Clear(ClearType::CurrentLine),
        cursor::Hide
    )?;

    queue_bottom_info_line(
        &mut stdout_lock,
        width,
        height,
        second_starting_index,
        second_display_offset,
        &dir_states,
    )?;

    stdout_lock.flush()?;

    Ok(())
}

// Redraw everything except the bottom info line.
fn redraw_upper(
    mut stdout_lock: &mut StdoutLock,
    win_pixels: &mut WindowPixels,
    runtime: &Runtime,
    mut image_handles: &mut Vec<ImageHandle>,
    dir_states: &DirStates,
    left_paths: &HashMap<std::path::PathBuf, DirLocation>,
    available_execs: &HashMap<&str, std::path::PathBuf>,
    second_starting_index: u16,
    second_display_offset: u16,
) -> crossterm::Result<()> {
    queue!(stdout_lock, terminal::Clear(ClearType::All))?;

    let (width, height) = terminal::size()?;
    let (second_column, column_bot_y, column_height) = calc_second_column_info(width, height);

    *win_pixels = get_win_pixels()?;

    queue_first_column(
        &mut stdout_lock,
        &dir_states,
        &left_paths,
        width,
        column_height,
        column_bot_y,
    )?;
    queue_second_column(
        &mut stdout_lock,
        second_column,
        width,
        column_bot_y,
        &dir_states.current_entries,
        second_display_offset,
        second_starting_index,
    )?;
    queue_third_column(
        stdout_lock,
        &runtime,
        &mut image_handles,
        &win_pixels,
        &dir_states,
        &left_paths,
        &available_execs,
        width,
        height,
        column_height,
        column_bot_y,
        (second_starting_index + second_display_offset) as usize,
    )?;

    Ok(())
}

fn queue_bottom_info_line(
    stdout_lock: &mut StdoutLock,
    width: u16,
    height: u16,
    second_starting_index: u16,
    second_display_offset: u16,
    dir_states: &DirStates,
) -> crossterm::Result<()> {
    if dir_states.current_entries.len() <= 0 {
        return Ok(());
    }

    let updated_second_entry_index = second_starting_index + second_display_offset;

    let updated_curr_entry = &dir_states.current_entries[(updated_second_entry_index) as usize];

    let permissions = &dir_states.current_entries[updated_second_entry_index as usize]
        .metadata
        .permissions();

    let naive = NaiveDateTime::from_timestamp(
        updated_curr_entry.metadata.mtime(),
        27, // Apparently 27 leap seconds have passed since 1972
    );

    let date_time: DateTime<Local> =
        DateTime::from_utc(naive, Local.offset_from_local_datetime(&naive).unwrap());

    // let display_date = date_time.format("%a %b %d %H:%M:%S %Y");
    let display_date = date_time.format("%c");

    let colored_mode = {
        let mut colored_mode = vec![];
        queue!(colored_mode, style::SetAttribute(Attribute::Bold))?;
        for (index, byte) in strmode(permissions.mode()).bytes().enumerate() {
            if index > 3 {
                queue!(colored_mode, style::SetAttribute(Attribute::Reset))?;
            }

            match &[byte] {
                b"d" => {
                    queue!(colored_mode, style::SetForegroundColor(Color::DarkBlue))?;
                    colored_mode.push(byte);
                }
                b"r" => {
                    queue!(colored_mode, style::SetForegroundColor(Color::DarkYellow))?;
                    colored_mode.push(byte);
                }
                b"w" => {
                    queue!(colored_mode, style::SetForegroundColor(Color::DarkRed))?;
                    colored_mode.push(byte);
                }
                b"x" => {
                    queue!(colored_mode, style::SetForegroundColor(Color::DarkGreen))?;
                    colored_mode.push(byte);
                }
                b"-" => {
                    queue!(colored_mode, style::SetForegroundColor(Color::DarkBlue))?;
                    colored_mode.push(byte);
                }
                b"l" => {
                    queue!(
                        colored_mode,
                        style::SetAttribute(Attribute::Reset),
                        style::SetForegroundColor(Color::DarkCyan),
                    )?;
                    colored_mode.push(byte);
                    queue!(colored_mode, style::SetAttribute(Attribute::Bold),)?;
                }
                b"c" | b"b" => {
                    queue!(colored_mode, style::SetForegroundColor(Color::DarkYellow),)?;
                    colored_mode.push(byte);
                }
                _ => {
                    queue!(colored_mode, style::SetForegroundColor(Color::Reset),)?;
                    colored_mode.push(byte);
                }
            }
        }
        queue!(colored_mode, style::SetForegroundColor(Color::Reset),)?;

        colored_mode
    };

    let colored_size = {
        let mut colored_size = vec![];
        queue!(
            colored_size,
            style::SetForegroundColor(Color::DarkGreen),
            style::SetAttribute(Attribute::Bold),
            style::Print(format!(
                "{:4}",
                human_size(updated_curr_entry.metadata.size())
            )),
            style::SetAttribute(Attribute::Reset),
        )?;
        colored_size
    };

    let colored_display_date = {
        let mut colored_display_date = vec![];
        queue!(
            colored_display_date,
            style::SetForegroundColor(Color::DarkBlue),
            style::Print(&display_date),
        )?;
        colored_display_date
    };

    // stdout_lock.flush()?;

    queue!(
        stdout_lock,
        style::SetAttribute(Attribute::Reset),
        cursor::MoveTo(0, height - 1),
        terminal::Clear(ClearType::CurrentLine),
        style::Print(std::str::from_utf8(&colored_mode).unwrap()),
        style::PrintStyledContent(
            format!(" {:2}", updated_curr_entry.metadata.nlink())
                .with(Color::DarkRed)
                .attribute(Attribute::Bold)
        ),
        style::PrintStyledContent(
            format!(
                " {}",
                unix_users::get_unix_username(updated_curr_entry.metadata.uid()).unwrap()
            )
            .with(Color::DarkYellow)
            .attribute(Attribute::Bold)
        ),
        style::PrintStyledContent(
            format!(
                " {}",
                unix_users::get_unix_groupname(updated_curr_entry.metadata.gid()).unwrap()
            )
            .with(Color::DarkYellow)
            .attribute(Attribute::Bold)
        ),
        style::Print(format!(
            " {} {}",
            std::str::from_utf8(&colored_size).unwrap(),
            std::str::from_utf8(&colored_display_date).unwrap(),
        )),
    )?;

    let display_position = format!(
        "{}/{}",
        updated_second_entry_index + 1,
        dir_states.current_entries.len()
    );

    queue!(
        stdout_lock,
        cursor::MoveTo(width - (display_position.len() as u16), height - 1),
        style::SetForegroundColor(Color::Reset),
        style::Print(display_position),
    )?;

    // queue!(
    //     stdout_lock,
    //     style::SetAttribute(Attribute::Reset),
    //     cursor::MoveTo(0, height - 1),
    //     terminal::Clear(ClearType::CurrentLine),
    //     style::Print(format!(
    //         "{} {:2} {} {} {:4} {}",
    //         strmode(permissions.mode()),
    //         updated_curr_entry.metadata.nlink(),
    //         unix_users::get_unix_username(updated_curr_entry.metadata.uid()).unwrap(),
    //         unix_users::get_unix_groupname(updated_curr_entry.metadata.gid()).unwrap(),
    //         human_size(updated_curr_entry.metadata.size()),
    //         display_date,
    //     )),
    // )?;

    // let display_position = format!(
    //     "{}/{}",
    //     updated_second_entry_index + 1,
    //     dir_states.current_entries.len()
    // );

    // queue!(
    //     stdout_lock,
    //     cursor::MoveTo(width - (display_position.len() as u16), height - 1),
    //     style::Print(display_position),
    // )?;

    Ok(())
}

// Handle for a task which displays an image
struct ImageHandle {
    handle: JoinHandle<crossterm::Result<()>>,
    can_display_image: Arc<Mutex<bool>>,
}

// Should get the hostname in a POSIX-compliant way.
// Only tested on Linux, however.
fn get_hostname() -> io::Result<String> {
    unsafe {
        // NOTE(Chris): HOST_NAME_MAX is defined in bits/local_lim.h on Linux

        let host_name_max: usize = libc::sysconf(libc::_SC_HOST_NAME_MAX) as usize;

        // HOST_NAME_MAX can't be larger than 256 on POSIX systems
        let mut name_buf = [0; 256];

        let err = libc::gethostname(name_buf.as_mut_ptr(), host_name_max);
        match err {
            0 => {
                // Make sure that at least the last character is NUL
                name_buf[host_name_max - 1] = 0;

                let null_position = name_buf.iter().position(|byte| *byte == 0).unwrap();

                let name_u8 = { &*(&mut name_buf[..] as *mut [i8] as *mut [u8]) };

                Ok(std::str::from_utf8(&name_u8[0..null_position])
                    .unwrap()
                    .to_string())
            }
            1 => {
                let errno_location = libc::__errno_location();
                let errno = (*errno_location) as i32;

                Err(io::Error::from_raw_os_error(errno))
            }
            _ => {
                panic!("Invalid libc:gethostname return value: {}", err);
            }
        }
    }
}

// A Linux-specific, possibly-safe wrapper around an ioctl call with TIOCGWINSZ.
// Gets the width and height of the terminal in pixels.
fn get_win_pixels() -> std::result::Result<WindowPixels, io::Error> {
    let win_pixels = unsafe {
        let mut winsize = libc::winsize {
            ws_col: 0,
            ws_row: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        // NOTE(Chris): From Linux's man ioctl_tty
        const TIOCGWINSZ: u64 = 21523;

        // 0 is the file descriptor for stdin
        // NOTE(Chris): This only works if stdin is a tty. If it is not (e.g. zsh widgets), then
        // you may have to redirect the tty to stdin.
        // Example:
        // rf() { rolf < $TTY }
        let err = libc::ioctl(0, TIOCGWINSZ, &mut winsize);
        if err != 0 {
            let errno_location = libc::__errno_location();
            let errno = (*errno_location) as i32;

            return Err(io::Error::from_raw_os_error(errno));

            // panic!("Failed to get the size of terminal window in pixels.");
        }

        WindowPixels {
            width: winsize.ws_xpixel,
            height: winsize.ws_ypixel,
        }
    };

    Ok(win_pixels)
}

fn calc_second_column_info(width: u16, height: u16) -> (u16, u16, u16) {
    let second_column = width / 6 + 1;
    // Represents the bottom-most y-cell of a column
    let column_bot_y = height - 2;
    // Represents the number of cells in a column vertically.
    let column_height = height - 2;

    (second_column, column_bot_y, column_height)
}

fn queue_all_columns(
    mut stdout_lock: &mut StdoutLock,
    runtime: &Runtime,
    mut image_handles: &mut HandlesVec,
    win_pixels: &WindowPixels,
    dir_states: &DirStates,
    left_paths: &HashMap<std::path::PathBuf, DirLocation>,
    available_execs: &HashMap<&str, std::path::PathBuf>,
    width: u16,
    height: u16,
    column_height: u16,
    column_bot_y: u16,
    second_column: u16,
    second_display_offset: u16,
    second_starting_index: u16,
) -> crossterm::Result<()> {
    queue_first_column(
        &mut stdout_lock,
        &dir_states,
        &left_paths,
        width,
        column_height,
        column_bot_y,
    )?;
    queue_second_column(
        &mut stdout_lock,
        second_column,
        width,
        column_bot_y,
        &dir_states.current_entries,
        second_display_offset,
        second_starting_index,
    )?;
    queue_third_column(
        stdout_lock,
        &runtime,
        &mut image_handles,
        &win_pixels,
        &dir_states,
        &left_paths,
        &available_execs,
        width,
        height,
        column_height,
        column_bot_y,
        (second_starting_index + second_display_offset) as usize,
    )?;

    queue_bottom_info_line(
        &mut stdout_lock,
        width,
        height,
        second_starting_index,
        second_display_offset,
        &dir_states,
    )?;

    Ok(())
}

fn queue_first_column(
    mut w: &mut StdoutLock,
    dir_states: &DirStates,
    left_paths: &HashMap<std::path::PathBuf, DirLocation>,
    width: u16,
    column_height: u16,
    column_bot_y: u16,
) -> crossterm::Result<()> {
    if let Some(prev_dir) = &dir_states.prev_dir {
        let (display_offset, starting_index) = find_correct_location(
            &left_paths,
            column_height,
            prev_dir,
            &dir_states.prev_entries,
            &dir_states.current_dir,
        );
        queue_entries_column(
            &mut w,
            1,
            width / 6 - 2,
            column_bot_y,
            &dir_states.prev_entries,
            display_offset,
            starting_index,
        )?;
    } else {
        queue_oneline_column(&mut w, 1, width / 6 - 2, column_bot_y, "")?;
    }
    Ok(())
}

// All this function actually does is call queue_entries_column, but it's here to match the naming
// scheme of queue_first_column and queue_third_column
fn queue_second_column(
    mut w: &mut StdoutLock,
    second_column: u16,
    width: u16,
    column_bot_y: u16,
    // dir_states: &DirStates,
    entries: &Vec<DirEntryInfo>,
    second_display_offset: u16,
    second_starting_index: u16,
) -> crossterm::Result<()> {
    queue_entries_column(
        &mut w,
        second_column,
        width / 2 - 2,
        column_bot_y,
        &entries,
        second_display_offset,
        second_starting_index,
    )?;

    Ok(())
}

fn queue_third_column(
    mut w: &mut StdoutLock,
    runtime: &Runtime,
    mut handles: &mut HandlesVec,
    win_pixels: &WindowPixels,
    dir_states: &DirStates,
    left_paths: &HashMap<std::path::PathBuf, DirLocation>,
    available_execs: &HashMap<&str, std::path::PathBuf>,
    width: u16,
    height: u16,
    column_height: u16,
    column_bot_y: u16,
    change_index: usize,
) -> crossterm::Result<()> {
    let left_x = width / 2 + 1;
    let right_x = width - 2;

    if dir_states.current_entries.len() <= 0 {
        queue_blank_column(&mut w, left_x, right_x, column_height)?;
    } else {
        let display_entry = &dir_states.current_entries[change_index];

        let file_type = display_entry.dir_entry.file_type().unwrap();

        if file_type.is_dir() {
            queue_third_column_dir(
                &mut w,
                &left_paths,
                width,
                left_x,
                right_x,
                column_bot_y,
                &display_entry,
            )?;
        } else if file_type.is_file() {
            queue_third_column_file(
                &mut w,
                &runtime,
                &mut handles,
                &display_entry,
                &available_execs,
                *win_pixels,
                width,
                height,
                column_height,
                column_bot_y,
                left_x,
                right_x,
            )?;
        } else if file_type.is_symlink() {
            // TODO(Chris): Show error if symlink is invalid
            match std::fs::metadata(display_entry.dir_entry.path()) {
                Ok(underlying_metadata) => {
                    let underlying_file_type = underlying_metadata.file_type();

                    if underlying_file_type.is_dir() {
                        queue_third_column_dir(
                            &mut w,
                            &left_paths,
                            width,
                            left_x,
                            right_x,
                            column_bot_y,
                            &display_entry,
                        )?;
                    } else if underlying_file_type.is_file() {
                        queue_third_column_file(
                            &mut w,
                            &runtime,
                            &mut handles,
                            &display_entry,
                            &available_execs,
                            *win_pixels,
                            width,
                            height,
                            column_height,
                            column_bot_y,
                            left_x,
                            right_x,
                        )?;
                    } else {
                        queue_blank_column(&mut w, left_x, right_x, column_height)?;
                    }
                }
                Err(_) => {
                    queue_blank_column(&mut w, left_x, right_x, column_height)?;
                }
            }
        } else {
            queue_blank_column(&mut w, left_x, right_x, column_height)?;
        }
    }

    Ok(())
}

fn queue_third_column_dir(
    mut w: &mut StdoutLock,
    left_paths: &HashMap<std::path::PathBuf, DirLocation>,
    width: u16,
    left_x: u16,
    right_x: u16,
    column_bot_y: u16,
    display_entry: &DirEntryInfo,
) -> crossterm::Result<()> {
    w.write(b"\x1b_Ga=d;\x1b\\")?; // Delete all visible images

    let third_dir = display_entry.dir_entry.path();

    match get_sorted_entries(&third_dir) {
        Ok(third_entries) => {
            let (display_offset, starting_index) = match left_paths.get(&third_dir) {
                Some(dir_location) => (dir_location.display_offset, dir_location.starting_index),
                None => (0, 0),
            };

            queue_entries_column(
                &mut w,
                width / 2 + 1,
                width - 2,
                column_bot_y,
                &third_entries,
                display_offset,
                starting_index,
            )?;
        }
        Err(err) => {
            let message = match err.kind() {
                io::ErrorKind::PermissionDenied => String::from("permission denied"),
                _ => {
                    format!("error reading: {}", err)
                }
            };

            queue_oneline_column(&mut w, left_x, right_x, column_bot_y, &message)?;
        }
    }

    Ok(())
}

fn queue_third_column_file(
    mut w: &mut StdoutLock,
    runtime: &Runtime,
    handles: &mut HandlesVec,
    display_entry: &DirEntryInfo,
    available_execs: &HashMap<&str, std::path::PathBuf>,
    win_pixels: WindowPixels,
    width: u16,
    height: u16,
    column_height: u16,
    column_bot_y: u16,
    left_x: u16,
    right_x: u16,
) -> crossterm::Result<()> {
    queue_blank_column(&mut w, left_x, right_x, column_height)?;

    let third_file = display_entry.dir_entry.path();

    match third_file.extension() {
        Some(os_str_ext) => match os_str_ext.to_str() {
            Some(ext) => {
                let ext = ext.to_lowercase();
                let ext = ext.as_str();

                match ext {
                    "png" | "jpg" | "jpeg" | "mp4" | "webm" | "mkv" => {
                        let can_display_image = Arc::new(Mutex::new(true));

                        queue!(
                            w,
                            style::SetAttribute(Attribute::Reset),
                            style::SetAttribute(Attribute::Reverse),
                            cursor::MoveTo(left_x, 1),
                            style::Print("Loading..."),
                            style::SetAttribute(Attribute::Reset),
                        )?;

                        w.flush()?;

                        let preview_image_handle = runtime.spawn(preview_image_or_video(
                            win_pixels.clone(),
                            third_file.clone(),
                            ext.to_string(),
                            width,
                            height,
                            left_x,
                            Arc::clone(&can_display_image),
                        ));

                        handles.push(ImageHandle {
                            handle: preview_image_handle,
                            can_display_image,
                        });
                    }
                    _ => match available_execs.get("highlight") {
                        None => (),
                        Some(highlight) => {
                            // TODO(Chris): Actually show that something went wrong
                            let output = Command::new(highlight)
                                .arg("-O")
                                .arg("ansi")
                                .arg("--max-size=500K")
                                .arg(&third_file)
                                .output()
                                .unwrap();

                            // TODO(Chris): Handle case when file is not valid utf8
                            if let Ok(text) = std::str::from_utf8(&output.stdout) {
                                let mut curr_y = 1; // Columns start at y = 1
                                queue!(&mut w, cursor::MoveTo(left_x, curr_y))?;

                                queue!(&mut w, terminal::DisableLineWrap)?;

                                for ch in text.as_bytes() {
                                    if curr_y > column_bot_y {
                                        break;
                                    }

                                    if *ch == b'\n' {
                                        curr_y += 1;

                                        queue!(&mut w, cursor::MoveTo(left_x, curr_y))?;
                                    } else {
                                        // NOTE(Chris): We write directly to stdout so as to
                                        // allow the ANSI escape codes to match the end of a
                                        // line
                                        w.write(&[*ch])?;
                                    }
                                }

                                queue!(&mut w, terminal::EnableLineWrap)?;

                                // TODO(Chris): Figure out why the right-most edge of the
                                // terminal sometimes has a character that should be one beyond
                                // that right-most edge. This bug occurs when right-most edge
                                // isn't blanked out (as is currently done below).

                                // Clear the right-most edge of the terminal, since it might
                                // have been drawn over when printing file contents
                                for curr_y in 1..=column_bot_y {
                                    queue!(
                                        &mut w,
                                        cursor::MoveTo(width, curr_y),
                                        style::Print(' ')
                                    )?;
                                }
                            }
                        }
                    },
                }
            }
            None => (),
        },
        None => (),
    }

    Ok(())
}

async fn preview_image_or_video(
    win_pixels: WindowPixels,
    third_file: PathBuf,
    ext: String,
    width: u16,
    height: u16,
    left_x: u16,
    can_display_image: Arc<Mutex<bool>>,
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
                    &input,
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
                    &input,
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

    if orig_img_cells_width != img_cells_width {
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
    let mut w = stdout.lock();

    let rgba = img.to_rgba8();
    let raw_img = rgba.as_raw();

    // eprintln!(
    //     "   ending - img_cells_width: {:3}, img_cells_height: {:3}",
    //     img_cells_width, img_cells_height
    // );

    // This scope exists to eventually unlock the mutex
    {
        let can_display_image = can_display_image.lock().unwrap();

        if *can_display_image {
            let path = store_in_tmp_file(raw_img)?;

            // execute!(
            //     w,
            //     cursor::MoveTo(left_x, 1),
            //     style::Print("Should display!")
            // )?;

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
        }
    }

    w.flush()?;

    // queue!(
    //     w,
    //     cursor::MoveTo(left_x, 21),
    //     style::Print("preview_image has finished.")
    // )?;

    w.flush()?;

    Ok(())
}

fn abort_image_handles(image_handles: &mut Vec<ImageHandle>) {
    while image_handles.len() > 0 {
        let image_handle = image_handles.pop().unwrap();
        let mut can_display_image = image_handle.can_display_image.lock().unwrap();
        *can_display_image = false;
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

#[derive(Debug, Clone, Copy)]
struct WindowPixels {
    width: u16,
    height: u16,
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
            "~/{}",
            dir_states
                .current_dir
                .strip_prefix(home_path)
                .unwrap()
                .to_str()
                .unwrap()
        )
    } else if let None = dir_states.prev_dir {
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
    parent_dir: &std::path::PathBuf,
    parent_entries: &Vec<DirEntryInfo>,
    dir: &std::path::PathBuf,
) -> (u16, u16) {
    return match left_paths.get(parent_dir) {
        Some(dir_location) => (dir_location.display_offset, dir_location.starting_index),
        None => {
            let first_bottom_index = column_height;

            let parent_entry_index = parent_entries
                .iter()
                .position(|entry| entry.dir_entry.path() == *dir)
                .unwrap();

            if parent_entry_index <= first_bottom_index as usize {
                (parent_entry_index as u16, 0)
            } else {
                // Center vaguely on parent_entry_index
                let down_offset = column_height / 2;

                (down_offset, (parent_entry_index as u16) - down_offset)
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

        dir_states.set_current_dir(std::env::var("PWD").unwrap())?;

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
struct DirEntryInfo {
    dir_entry: DirEntry,
    metadata: Metadata,
}

fn cmp_dir_entry_info(entry_info_1: &DirEntryInfo, entry_info_2: &DirEntryInfo) -> Ordering {
    cmp_dir_entry(&entry_info_1.dir_entry, &entry_info_2.dir_entry)
}

// Sorts std::fs::DirEntry by file type first (with directory coming before files),
// then by file name. Symlinks are ignored in favor of the original files' file types.
// lf seems to do this with symlinks as well.
// TODO(Chris): Get rid of all the zany unwrap() calls in this function, since it's not supposed to
// fail
fn cmp_dir_entry(entry1: &DirEntry, entry2: &DirEntry) -> Ordering {
    let file_type1 = match std::fs::metadata(entry1.path()) {
        Ok(metadata) => metadata.file_type(),
        Err(err) => {
            match err.kind() {
                // Just use name of symbolic link
                io::ErrorKind::NotFound => entry1.metadata().unwrap().file_type(),
                _ => panic!("{}", err),
            }
        }
    };
    let file_type2 = match std::fs::metadata(entry2.path()) {
        Ok(metadata) => metadata.file_type(),
        Err(err) => {
            match err.kind() {
                // Just use name of symbolic link
                io::ErrorKind::NotFound => entry2.metadata().unwrap().file_type(),
                _ => panic!("{}", err),
            }
        }
    };

    if file_type1.is_dir() && file_type2.is_file() {
        return Ordering::Less;
    } else if file_type2.is_dir() && file_type1.is_file() {
        return Ordering::Greater;
    } else {
        return cmp_natural(
            entry1.file_name().to_str().unwrap(),
            entry2.file_name().to_str().unwrap(),
        );
    }
}

fn save_location(
    left_paths: &mut HashMap<
        std::path::PathBuf,
        DirLocation,
        std::collections::hash_map::RandomState,
    >,
    dir_states: &DirStates,
    second_entry_index: u16,
    second_starting_index: u16,
    second_display_offset: u16,
) {
    left_paths.insert(
        dir_states.current_dir.clone(),
        DirLocation {
            dir_path: dir_states.current_entries[second_entry_index as usize]
                .dir_entry
                .path(),
            starting_index: second_starting_index,
            display_offset: second_display_offset,
        },
    );
}

fn update_entries_column(
    w: &mut io::StdoutLock,
    left_x: u16,
    right_x: u16,
    column_bot_y: u16,
    entries: &Vec<DirEntryInfo>,
    old_offset: u16,
    old_start_index: u16,
    new_offset: u16,
    new_start_index: u16,
) -> crossterm::Result<()> {
    if new_start_index != old_start_index {
        queue_entries_column(
            w,
            left_x,
            right_x,
            column_bot_y,
            entries,
            new_offset,
            new_start_index,
        )?;
        return Ok(());
    }

    queue!(w, style::SetAttribute(Attribute::Reset))?;

    // Update the old offset
    queue_full_entry(w, &entries, left_x, right_x, old_offset, old_start_index)?;

    // Update the new offset
    queue!(w, style::SetAttribute(Attribute::Reverse))?;

    queue_full_entry(w, &entries, left_x, right_x, new_offset, new_start_index)?;

    queue!(w, style::SetAttribute(Attribute::NoReverse))?;

    Ok(())
}

fn queue_full_entry(
    w: &mut io::StdoutLock,
    entries: &Vec<DirEntryInfo>,
    left_x: u16,
    right_x: u16,
    display_offset: u16,
    starting_index: u16,
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
    let mut curr_x = left_x; // This is the cell which we are about to print into

    queue!(
        w,
        cursor::MoveTo(left_x, display_offset + 1),
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

    Ok(())
}

fn queue_entries_column(
    w: &mut io::StdoutLock,
    left_x: u16,
    right_x: u16,
    bottom_y: u16,
    entries: &Vec<DirEntryInfo>,
    offset: u16,
    start_index: u16, // Index to start with in entries
) -> crossterm::Result<()> {
    let mut curr_y = 1; // 1 is the starting y for columns

    queue!(w, style::SetAttribute(Attribute::Reset))?;
    if entries.len() <= 0 {
        queue!(
            w,
            cursor::MoveTo(left_x, curr_y),
            style::Print(" "),
            style::SetAttribute(Attribute::Reverse),
            style::SetForegroundColor(Color::White),
            style::Print("empty"),
            style::SetAttribute(Attribute::Reset),
            style::Print(" "),
        )?;

        let mut curr_x = left_x + 7; // Length of " empty "

        while curr_x <= right_x {
            queue!(w, style::Print(' '))?;

            curr_x += 1;
        }

        curr_y += 1;
    } else {
        let our_entries = &entries[start_index as usize..];
        for _entry in our_entries {
            if curr_y > bottom_y {
                break;
            }

            let is_curr_entry = curr_y - 1 == offset;

            if is_curr_entry {
                queue!(w, style::SetAttribute(Attribute::Reverse))?;
            }

            queue_full_entry(w, &entries, left_x, right_x, curr_y - 1, start_index)?;

            if is_curr_entry {
                queue!(w, style::SetAttribute(Attribute::Reset))?;
            }

            curr_y += 1;
        }
    }

    let col_width = right_x - left_x + 1;

    // Ensure that the bottom of "short buffers" are properly cleared
    while curr_y <= bottom_y {
        queue!(w, cursor::MoveTo(left_x, curr_y))?;

        for _ in 0..col_width {
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
    // https://sw.kovidgoyal.net/kitty/graphics-protocol/#deleting-images
    let draw_beginning = b"\x1b_Ga=d;\x1b\\"; // Delete all visible images
    w.write(draw_beginning)?;

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
    let mut entries = std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let dir_entry = entry.unwrap();
            let metadata = std::fs::symlink_metadata(dir_entry.path()).unwrap();

            DirEntryInfo {
                dir_entry,
                metadata,
            }
        })
        .collect::<Vec<DirEntryInfo>>();

    entries.sort_by(cmp_dir_entry_info);

    Ok(entries)
}
