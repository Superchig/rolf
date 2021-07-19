mod natural_sort; // This declares the existence of the natural_sort module, which searches by
                  // default for natural_sort.rs or natural_sort/mod.rs

use open;

use natural_sort::cmp_natural;
use std::cmp::Ordering;
use std::collections::hash_map::HashMap;
use std::fs::DirEntry;
use std::io::{self, Stdout, Write};
use std::path::Path;
use std::process::Command;
use std::vec::Vec;

use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute, queue,
    style::{self, Attribute, Color},
    terminal::{self, ClearType},
};

fn main() -> crossterm::Result<()> {
    let mut w = io::stdout();

    terminal::enable_raw_mode()?;

    queue!(w, terminal::EnterAlternateScreen)?;

    let result = run(&mut w);

    execute!(
        w,
        style::ResetColor,
        cursor::Show,
        terminal::LeaveAlternateScreen,
        cursor::MoveToNextLine(1),
    )?;

    terminal::disable_raw_mode()?;

    match result {
        Ok(_) => println!("Goodbye."),
        Err(err) => panic!(err),
    }

    Ok(())
}

fn run(mut w: &mut io::Stdout) -> crossterm::Result<()> {
    let user_name = match std::env::var("USER") {
        Ok(val) => val,
        Err(e) => panic!("Could not read $USER environment variable: {}", e),
    };

    // TODO(Chris): Read the hostname in via POSIX syscalls
    // https://man7.org/linux/man-pages/man2/gethostname.2.html
    let host_name = match std::env::var("HOSTNAME") {
        Ok(val) => val,
        Err(e) => panic!("Could not read $HOSTNAME environment variable: {}", e),
    };

    let home_name = match std::env::var("HOME") {
        Ok(val) => val,
        Err(e) => panic!("Could not read $HOME: {}", e),
    };

    let home_path = Path::new(&home_name[..]);

    // NOTE(Chris): The default column ratio is 1:2:3

    let mut dir_states = DirStates::new()?;

    dir_states.set_current_dir(".")?;

    let mut second_display_offset = 0;

    // FIXME(Chris): Consider refactoring these weird flags into functions?
    // FIXME(Chris): Eliminate the flags entirely and replace them with calls to their relevant
    // functions

    let mut is_first_iteration = true;

    let mut second_starting_index = 0;

    let mut left_paths: HashMap<std::path::PathBuf, DirLocation> = HashMap::new();

    queue!(
        w,
        style::ResetColor,
        terminal::Clear(ClearType::All),
        cursor::Hide,
    )?;

    // Main input loop
    loop {
        let current_dir_display = format_current_dir(&dir_states, home_path);

        let second_entry_index = second_starting_index + second_display_offset;

        let curr_entry;
        let file_stem = if dir_states.current_entries.len() <= 0 {
            ""
        } else {
            curr_entry = dir_states.current_entries[second_entry_index as usize].file_name();
            curr_entry.to_str().unwrap()
        };

        queue!(
            w,
            cursor::MoveTo(0, 0),
            terminal::Clear(ClearType::CurrentLine),
            style::SetForegroundColor(Color::DarkGreen),
            style::SetAttribute(Attribute::Bold),
            style::Print(format!("{}@{}", user_name, host_name)),
            style::SetForegroundColor(Color::White),
            style::Print(":"),
            style::SetForegroundColor(Color::DarkBlue),
            style::Print(format!("{}/", current_dir_display)),
            style::SetForegroundColor(Color::White),
            style::Print(file_stem),
        )?;

        // The terminal's height is also the index of the lowest cell
        let (width, height) = terminal::size()?;
        let second_column = width / 6 + 1;
        let column_bot_y = height - 2;
        let column_height = column_bot_y - 1;

        if is_first_iteration {
            queue_first_column(
                &mut w,
                &dir_states,
                &left_paths,
                width,
                column_height,
                column_bot_y,
            )?;

            queue_second_column(
                &mut w,
                second_column,
                width,
                column_bot_y,
                &dir_states.current_entries,
                second_display_offset,
                second_starting_index,
            )?;

            // FIXME(Chris): Check that this function call is handled correctly
            queue_third_column(
                w,
                &dir_states,
                &left_paths,
                width,
                column_height,
                column_bot_y,
                0,
            )?;

            is_first_iteration = false;
        }

        w.flush()?;

        let second_bottom_index = second_starting_index + column_height;

        let mut enter_entry = || -> crossterm::Result<()> {
            if dir_states.current_entries.len() > 0 {
                save_location(
                    &mut left_paths,
                    &dir_states,
                    second_entry_index,
                    second_starting_index,
                    second_display_offset,
                );

                let selected_entry = &dir_states.current_entries[second_entry_index as usize];

                let selected_file_type = selected_entry.file_type().unwrap();

                if selected_file_type.is_dir() {
                    let selected_dir_path = selected_entry.path();

                    // FIXME(Chris): Avoid substituting apparent path with symlink target when
                    // entering symlinked directories
                    dir_states.set_current_dir(&selected_dir_path)?;

                    match left_paths.get(&selected_dir_path) {
                        Some(dir_location) => {
                            let curr_entry_index = dir_states
                                .current_entries
                                .iter()
                                .position(|entry| entry.path() == *dir_location.dir_path);

                            match curr_entry_index {
                                Some(curr_entry_index) => {
                                    let orig_entry_index = (dir_location.starting_index
                                        + dir_location.display_offset)
                                        as usize;
                                    if curr_entry_index == orig_entry_index {
                                        second_starting_index = dir_location.starting_index;
                                        second_display_offset = dir_location.display_offset;
                                    } else {
                                        second_starting_index = (curr_entry_index / 2) as u16;
                                        second_display_offset =
                                            (curr_entry_index as u16) - second_starting_index;
                                    }
                                }
                                None => {
                                    second_starting_index = 0;
                                    second_display_offset = 0;
                                }
                            }
                        }
                        None => {
                            second_starting_index = 0;
                            second_display_offset = 0;
                        }
                    };

                    queue_first_column(
                        &mut w,
                        &dir_states,
                        &left_paths,
                        width,
                        column_height,
                        column_bot_y,
                    )?;
                    queue_second_column(
                        &mut w,
                        second_column,
                        width,
                        column_bot_y,
                        &dir_states.current_entries,
                        second_display_offset,
                        second_starting_index,
                    )?;
                    queue_third_column(
                        w,
                        &dir_states,
                        &left_paths,
                        width,
                        column_height,
                        column_bot_y,
                        (second_starting_index + second_display_offset) as usize,
                    )?;
                } else if selected_file_type.is_file() {
                    open::that(selected_entry.path())?;
                }
            }

            Ok(())
        };

        match event::read()? {
            Event::Key(event) => match event.code {
                KeyCode::Char(ch) => {
                    match ch {
                        'q' => break,
                        // TODO(Chris): Account for possibility of no .parent() AKA when
                        // current_dir is '/'
                        'h' => {
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

                            dir_states.set_current_dir("..")?;

                            let (display_offset, starting_index) = find_correct_location(
                                &left_paths,
                                column_height,
                                &dir_states.current_dir,
                                &dir_states.current_entries,
                                &old_current_dir,
                            );
                            second_display_offset = display_offset;
                            second_starting_index = starting_index;

                            // TODO(Chris): Consider combining these two flags into one, since we're not using
                            // them separately
                            queue_first_column(
                                &mut w,
                                &dir_states,
                                &left_paths,
                                width,
                                column_height,
                                column_bot_y,
                            )?;
                            queue_second_column(
                                &mut w,
                                second_column,
                                width,
                                column_bot_y,
                                &dir_states.current_entries,
                                second_display_offset,
                                second_starting_index,
                            )?;
                            queue_third_column(
                                w,
                                &dir_states,
                                &left_paths,
                                width,
                                column_height,
                                column_bot_y,
                                (second_starting_index + second_display_offset) as usize,
                            )?;
                        }
                        'l' => {
                            enter_entry()?;
                        }
                        'j' => {
                            if dir_states.current_entries.len() > 0
                                && (second_entry_index as usize)
                                    < dir_states.current_entries.len() - 1
                            {
                                let old_starting_index = second_starting_index;
                                let old_display_offset = second_display_offset;

                                if second_display_offset >= (column_bot_y * 2 / 3)
                                    && (second_bottom_index as usize)
                                        < dir_states.current_entries.len() - 1
                                {
                                    second_starting_index += 1;
                                } else if second_entry_index != second_bottom_index {
                                    second_display_offset += 1;
                                }

                                update_entries_column(
                                    w,
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
                                    w,
                                    &dir_states,
                                    &left_paths,
                                    width,
                                    column_height,
                                    column_bot_y,
                                    (second_starting_index + second_display_offset) as usize,
                                )?;
                            }
                        }
                        'k' => {
                            if dir_states.current_entries.len() > 0 {
                                let old_starting_index = second_starting_index;
                                let old_display_offset = second_display_offset;

                                if second_display_offset <= (column_bot_y * 1 / 3)
                                    && second_starting_index > 0
                                {
                                    second_starting_index -= 1;
                                } else if second_entry_index > 0 {
                                    second_display_offset -= 1;
                                }

                                update_entries_column(
                                    w,
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
                                    w,
                                    &dir_states,
                                    &left_paths,
                                    width,
                                    column_height,
                                    column_bot_y,
                                    (second_starting_index + second_display_offset) as usize,
                                )?;
                            }
                        }
                        'e' => {
                            let editor = match std::env::var("VISUAL") {
                                Err(std::env::VarError::NotPresent) => {
                                    match std::env::var("EDITOR") {
                                        Err(std::env::VarError::NotPresent) => String::from(""),
                                        Err(err) => panic!(err),
                                        Ok(editor) => editor,
                                    }
                                }
                                Err(err) => panic!(err),
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
                                        .path()
                                        .to_str()
                                        .expect("Failed to convert path to string")
                                );

                                queue!(w, terminal::LeaveAlternateScreen)?;

                                Command::new("sh")
                                    .arg("-c")
                                    .arg(shell_command)
                                    .status()
                                    .expect("failed to execute editor command");

                                queue!(w, terminal::EnterAlternateScreen)?;

                                // FIXME(Chris): Refactor this into a closure, I guess
                                queue_first_column(
                                    &mut w,
                                    &dir_states,
                                    &left_paths,
                                    width,
                                    column_height,
                                    column_bot_y,
                                )?;
                                queue_second_column(
                                    &mut w,
                                    second_column,
                                    width,
                                    column_bot_y,
                                    &dir_states.current_entries,
                                    second_display_offset,
                                    second_starting_index,
                                )?;
                                queue_third_column(
                                    w,
                                    &dir_states,
                                    &left_paths,
                                    width,
                                    column_height,
                                    column_bot_y,
                                    (second_starting_index + second_display_offset) as usize,
                                )?;
                            }
                        }
                        _ => (),
                    }
                }
                KeyCode::Enter => enter_entry()?,
                _ => (),
            },
            Event::Mouse(_) => (),
            Event::Resize(_, _) => {
                queue!(w, terminal::Clear(ClearType::All))?;

                queue_first_column(
                    &mut w,
                    &dir_states,
                    &left_paths,
                    width,
                    column_height,
                    column_bot_y,
                )?;
                queue_second_column(
                    &mut w,
                    second_column,
                    width,
                    column_bot_y,
                    &dir_states.current_entries,
                    second_display_offset,
                    second_starting_index,
                )?;
                queue_third_column(
                    w,
                    &dir_states,
                    &left_paths,
                    width,
                    column_height,
                    column_bot_y,
                    (second_starting_index + second_display_offset) as usize,
                )?;
            }
        }
    }

    Ok(())
}

fn queue_first_column(
    mut w: &mut Stdout,
    dir_states: &DirStates,
    left_paths: &HashMap<std::path::PathBuf, DirLocation>,
    width: u16,
    column_height: u16,
    column_bot_y: u16,
) -> crossterm::Result<()> {
    let (display_offset, starting_index) = find_correct_location(
        &left_paths,
        column_height,
        &dir_states.prev_dir,
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

    Ok(())
}

// All this function actually does is call queue_entries_column, but it's here to match the naming
// scheme of queue_first_column and queue_third_column
fn queue_second_column(
    mut w: &mut Stdout,
    second_column: u16,
    width: u16,
    column_bot_y: u16,
    // dir_states: &DirStates,
    entries: &Vec<DirEntry>,
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
    mut w: &mut Stdout,
    dir_states: &DirStates,
    left_paths: &HashMap<std::path::PathBuf, DirLocation>,
    width: u16,
    column_height: u16,
    column_bot_y: u16,
    change_index: usize,
) -> crossterm::Result<()> {
    if dir_states.current_entries.len() <= 0 {
        queue_blank_column(&mut w, width / 2 + 1, width - 2, column_height)?;
    } else {
        let potential_third_dir = &dir_states.current_entries[change_index];

        if potential_third_dir.file_type().unwrap().is_dir() {
            let third_dir = potential_third_dir.path();
            let third_entries = get_sorted_entries(&third_dir);

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
        } else {
            queue_blank_column(&mut w, width / 2 + 1, width - 2, column_height)?;
        }
    }

    Ok(())
}

fn format_current_dir(dir_states: &DirStates, home_path: &Path) -> String {
    // TODO(Chris): Handle case when current_dir is '/'
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
    parent_entries: &Vec<DirEntry>,
    dir: &std::path::PathBuf,
) -> (u16, u16) {
    return match left_paths.get(parent_dir) {
        Some(dir_location) => (dir_location.display_offset, dir_location.starting_index),
        None => {
            let first_bottom_index = column_height;

            let parent_entry_index = parent_entries
                .iter()
                .position(|entry| entry.path() == *dir)
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
    current_entries: Vec<DirEntry>,
    prev_dir: std::path::PathBuf,
    prev_entries: Vec<DirEntry>,
}

impl DirStates {
    fn new() -> crossterm::Result<DirStates> {
        let current_dir = std::env::current_dir()?;

        let entries = get_sorted_entries(&current_dir);

        let prev_dir = current_dir.parent().unwrap().to_path_buf();

        let prev_entries = get_sorted_entries(&prev_dir);

        Ok(DirStates {
            current_dir,
            current_entries: entries,
            prev_dir,
            prev_entries,
        })
    }

    fn set_current_dir<P: AsRef<Path>>(self: &mut DirStates, path: P) -> crossterm::Result<()> {
        std::env::set_current_dir(path)?;

        self.current_dir = std::env::current_dir()?;

        self.current_entries = get_sorted_entries(&self.current_dir);

        // TODO(Chris): Handle case where there is no prev_dir (this results in an Option)
        self.prev_dir = self.current_dir.parent().unwrap().to_path_buf();

        self.prev_entries = get_sorted_entries(&self.prev_dir);

        Ok(())
    }
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
                _ => panic!(err),
            }
        }
    };
    let file_type2 = match std::fs::metadata(entry2.path()) {
        Ok(metadata) => metadata.file_type(),
        Err(err) => {
            match err.kind() {
                // Just use name of symbolic link
                io::ErrorKind::NotFound => entry2.metadata().unwrap().file_type(),
                _ => panic!(err),
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
            dir_path: dir_states.current_entries[second_entry_index as usize].path(),
            starting_index: second_starting_index,
            display_offset: second_display_offset,
        },
    );
}

fn update_entries_column(
    w: &mut io::Stdout,
    left_x: u16,
    right_x: u16,
    column_bot_y: u16,
    entries: &Vec<DirEntry>,
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
    w: &mut io::Stdout,
    entries: &Vec<DirEntry>,
    left_x: u16,
    right_x: u16,
    display_offset: u16,
    starting_index: u16,
) -> crossterm::Result<()> {
    let new_entry_index = starting_index + display_offset;
    let new_entry = &entries[new_entry_index as usize];
    let new_file_type = std::fs::symlink_metadata(new_entry.path())?.file_type();

    if new_file_type.is_dir() {
        queue!(
            w,
            style::SetForegroundColor(Color::DarkBlue),
            style::SetAttribute(Attribute::Bold),
        )?;
    } else if new_file_type.is_file() {
        queue!(w, style::SetForegroundColor(Color::White))?;
    } else if new_file_type.is_symlink() {
        queue!(
            w,
            style::SetForegroundColor(Color::DarkCyan),
            style::SetAttribute(Attribute::Bold)
        )?;
    }

    // TODO(Chris): Inline this function, since it's only used once
    queue_entry(
        w,
        left_x,
        right_x,
        display_offset,
        new_entry.file_name().to_str().unwrap(),
    )?;

    if new_file_type.is_dir() || new_file_type.is_symlink() {
        queue!(w, style::SetAttribute(Attribute::NormalIntensity))?;
    }

    Ok(())
}

fn queue_entries_column(
    w: &mut io::Stdout,
    left_x: u16,
    right_x: u16,
    bottom_y: u16,
    entries: &Vec<DirEntry>,
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

// This inherits the cursor's current y
fn queue_entry(
    w: &mut io::Stdout,
    left_x: u16,
    right_x: u16,
    display_offset: u16,
    file_name: &str,
) -> crossterm::Result<()> {
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

    Ok(())
}

fn queue_blank_column(
    w: &mut io::Stdout,
    left_x: u16,
    right_x: u16,
    column_height: u16,
) -> crossterm::Result<()> {
    let mut curr_y = 1; // 1 is the starting y for columns

    while curr_y < column_height {
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

fn get_sorted_entries<P: AsRef<Path>>(path: P) -> Vec<DirEntry> {
    let mut entries = std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect::<std::vec::Vec<std::fs::DirEntry>>();

    entries.sort_by(cmp_dir_entry);

    entries
}
