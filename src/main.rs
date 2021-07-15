mod natural_sort; // This declares the existence of the natural_sort module, which searches by
                  // default for natural_sort.rs or natural_sort/mod.rs

use natural_sort::cmp_natural;
use std::cmp::Ordering;
use std::collections::hash_map::HashMap;
use std::error::Error;
use std::fs::DirEntry;
use std::io::{self, Write};
use std::path::Path;
use std::vec::Vec;

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent},
    execute, queue,
    style::{self, Attribute, Color},
    terminal::{self, ClearType},
};

fn main() -> crossterm::Result<()> {
    let mut w = io::stdout();

    terminal::enable_raw_mode()?;

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

    let mut prev_entry_index = dir_states
        .prev_entries
        .iter()
        .position(|entry| entry.path() == dir_states.current_dir)
        .unwrap();

    // TODO(Chris): Consider refactoring these weird flags into functions?

    let mut first_column_changed = true;

    let mut second_column_changed = true;

    let mut second_starting_index = 0;

    let mut left_paths = HashMap::new();

    queue!(
        w,
        style::ResetColor,
        terminal::Clear(ClearType::All),
        cursor::Hide,
    )?;

    // Main input loop
    loop {
        // TODO(Chris): Handle case when current_dir is '/'
        // NOTE(Chris): This creates a new String, and it'd be nice to avoid making a heap
        // allocation here, but it's probably not worth trying to figure out how to use only a str
        let current_dir_display = if dir_states.current_dir.starts_with(home_path) {
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
        };

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

        // TODO(Chris): Correctly display previous directory column, especially
        // as it relates to the current path.

        if first_column_changed {
            queue_entries_column(
                &mut w,
                1,
                width / 6 - 2,
                column_bot_y,
                &dir_states.prev_entries,
                prev_entry_index as u16,
                0,
            )?;

            first_column_changed = false;
        }

        if second_column_changed {
            queue_entries_column(
                &mut w,
                second_column,
                width / 2 - 2,
                column_bot_y,
                &dir_states.current_entries,
                second_display_offset,
                second_starting_index,
            )?;

            second_column_changed = false;
        }

        w.flush()?;

        let column_height = column_bot_y - 1;
        let second_bottom_index = second_starting_index + column_height;

        match read_char()? {
            'q' => break,
            // TODO(Chris): Account for possibility of no .parent() AKA when
            // current_dir is '/'
            'h' => {
                let old_current_dir = dir_states.current_dir.clone();
                if dir_states.current_entries.len() > 0 {
                    left_paths.insert(
                        dir_states.current_dir.clone(),
                        DirLocation {
                            dir_path: dir_states.current_entries[second_entry_index as usize]
                                .path(),
                            starting_index: second_starting_index,
                            display_offset: second_display_offset,
                        },
                    );
                }

                dir_states.set_current_dir("..")?;

                // TODO(Chris): Refactor into a function
                prev_entry_index = dir_states
                    .prev_entries
                    .iter()
                    .position(|entry| entry.path() == dir_states.current_dir)
                    .unwrap();

                let curr_entry_index = dir_states
                    .current_entries
                    .iter()
                    .position(|entry| entry.path() == old_current_dir)
                    .unwrap();

                // TODO(Chris): Refactor into a function
                if curr_entry_index >= column_height as usize {
                    second_starting_index = (curr_entry_index / 2) as u16;
                    second_display_offset = (curr_entry_index as u16) - second_starting_index;
                } else {
                    second_starting_index = 0;
                    second_display_offset = curr_entry_index as u16;
                }

                // TODO(Chris): Consider combining these two flags into one, since we're not using
                // them separately
                first_column_changed = true;
                second_column_changed = true;
            }
            'l' => {
                if dir_states.current_entries.len() > 0 {
                    let selected_dir_path =
                        dir_states.current_entries[second_entry_index as usize].path();

                    dir_states.set_current_dir(&selected_dir_path)?;

                    prev_entry_index = dir_states
                        .prev_entries
                        .iter()
                        .position(|entry| entry.path() == dir_states.current_dir)
                        .unwrap();

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

                    first_column_changed = true;
                    second_column_changed = true;
                }
            }
            'j' => {
                if dir_states.current_entries.len() > 0
                    && (second_entry_index as usize) < dir_states.current_entries.len() - 1
                {
                    let old_starting_index = second_starting_index;
                    let old_display_offset = second_display_offset;

                    if second_display_offset >= (column_bot_y * 2 / 3)
                        && (second_bottom_index as usize) < dir_states.current_entries.len() - 1
                    {
                        second_starting_index += 1;
                    } else if second_entry_index != second_bottom_index {
                        second_display_offset += 1;
                    }

                    update_entries_column(
                        w,
                        second_column,
                        width / 2 - 2,
                        &dir_states.current_entries,
                        old_display_offset,
                        old_starting_index,
                        second_display_offset,
                        second_starting_index,
                    )?;
                }
            }
            'k' => {
                if dir_states.current_entries.len() > 0 {
                    let old_starting_index = second_starting_index;
                    let old_display_offset = second_display_offset;

                    if second_display_offset <= (column_bot_y * 1 / 3) && second_starting_index > 0
                    {
                        second_starting_index -= 1;
                    } else if second_entry_index > 0 {
                        second_display_offset -= 1;
                    }

                    update_entries_column(
                        w,
                        second_column,
                        width / 2 - 2,
                        &dir_states.current_entries,
                        old_display_offset,
                        old_starting_index,
                        second_display_offset,
                        second_starting_index,
                    )?;
                }
            }
            _ => (),
        }
    }

    Ok(())
}

struct DirLocation {
    dir_path: std::path::PathBuf,
    starting_index: u16,
    display_offset: u16,
}

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

    // TODO(Chris): Check out if io::Result works rather than crossterm::Result
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

fn read_char() -> crossterm::Result<char> {
    loop {
        if let Ok(Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            ..
        })) = event::read()
        {
            return Ok(c);
        }
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

fn update_entries_column(
    w: &mut io::Stdout,
    left_x: u16,
    right_x: u16,
    entries: &Vec<DirEntry>,
    old_offset: u16,
    old_start_index: u16,
    new_offset: u16,
    new_start_index: u16,
) -> crossterm::Result<()> {
    if new_start_index != old_start_index {
        // TODO(Chris): Find a way to avoid copy-pasting the height here (from a much earlier
        // queue_entries_column call)
        let (_width, height) = terminal::size()?;
        let column_bot_y = height - 2;
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
    new_offset: u16,
    new_start_index: u16,
) -> crossterm::Result<()> {
    let new_entry_index = new_start_index + new_offset;
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

    queue!(w, cursor::MoveTo(left_x, new_offset + 1), style::Print(' '))?; // 1 is the starting y for columns

    // TODO(Chris): Inline this function, since it's only used once
    queue_entry(w, left_x, right_x, new_entry.file_name().to_str().unwrap())?;

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

    let col_width = right_x - left_x;

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

// This inherits the cursor's current y
fn queue_entry(
    w: &mut io::Stdout,
    left_x: u16,
    right_x: u16,
    file_name: &str,
) -> crossterm::Result<()> {
    // Print as much of the file name as possible, truncating with '~' if necessary
    for (index, ch) in file_name.char_indices() {
        if (left_x as usize) + index >= (right_x as usize) - 2 {
            queue!(w, style::Print('~'),)?;
            break;
        }

        queue!(w, style::Print(ch),)?;
    }

    // Ensure that there are spaces printed at the end of the file name
    if (left_x as usize) + file_name.len() >= (right_x as usize) - 2 {
        queue!(w, style::Print(' '))?;
    } else {
        // This conversion is fine since file_name.len() can't be longer than
        // the terminal width in this instance.
        let mut curr_x = left_x + (file_name.len() as u16);

        while curr_x < right_x {
            queue!(w, style::Print(' '))?;

            curr_x += 1;
        }
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

// TODO(Chris): Move this assertion (and possibly a related assert macro) into another file

#[derive(Debug)]
struct AssertionError {
    description: String,
}

impl Error for AssertionError {
    fn description(&self) -> &str {
        &self.description
    }
}

impl std::fmt::Display for AssertionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Assertion failed: ")
    }
}

// TODO(Chris): Put this test and the cmp_alphanum function in its own
// file
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmp_alphanum_works() {
        assert_eq!(cmp_natural("10.bak", "1.bak"), Ordering::Greater);
        assert_eq!(cmp_natural("1.bak", "10.bak"), Ordering::Less);

        assert_eq!(cmp_natural("2.bak", "10.bak"), Ordering::Less);

        assert_eq!(cmp_natural("1.bak", "Cargo.lock"), Ordering::Less);

        assert_eq!(cmp_natural(".gitignore", "src"), Ordering::Less);

        assert_eq!(cmp_natural(".gitignore", ".gitignore"), Ordering::Equal);
    }

    #[test]
    fn scratch() {
        println!("{:#?}", "C".cmp("1"));
        assert!(true);

        assert_eq!(false, true);
    }
}
