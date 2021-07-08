mod natural_sort; // This declares the exiswtence of the natural_sort module, which searches by
                  // default for natural_sort.rs or natural_sort/mod.rs

use std::cmp::Ordering;
use std::fs::DirEntry;
use std::io::{self, Write};
use std::path::Path;
use std::vec::Vec;
use natural_sort::cmp_natural;

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent},
    execute, queue,
    style::{self, Attribute, Color},
    terminal::{self, ClearType},
};

fn main() -> crossterm::Result<()> {
    let mut w = io::stdout();

    execute!(w, terminal::EnterAlternateScreen)?;

    terminal::enable_raw_mode()?;

    let result = run(&mut w);

    execute!(
        w,
        style::ResetColor,
        cursor::Show,
        terminal::LeaveAlternateScreen,
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

    let mut entry_index = 0;

    // TODO(Chris): Eliminate a bunch of unwrap() calls by actually handling errors

    let mut current_dir = std::env::current_dir()?;

    let mut entries = get_sorted_entries(&current_dir);

    // TODO(Chris): Eliminate prev_dir variable entirely, since it seems to be unnecessary
    let mut prev_dir = current_dir.parent().unwrap().to_path_buf();

    let mut prev_entries = get_sorted_entries(&prev_dir);

    // Main input loop
    loop {
        // TODO(Chris): Handle case when current_dir is '/'
        // NOTE(Chris): This creates a new String, and it'd be nice to avoid making a heap
        // allocation here, but it's probably not worth trying to figure out how to use only a str
        let current_dir_display = if current_dir.starts_with(home_path) {
            // "~"
            format!(
                "~/{}",
                current_dir
                    .strip_prefix(home_path)
                    .unwrap()
                    .to_str()
                    .unwrap()
            )
        } else {
            current_dir.to_str().unwrap().to_string()
        };

        let curr_entry;
        let file_stem = if entries.len() <= 0 {
            ""
        } else {
            curr_entry = entries[entry_index as usize].file_name();
            curr_entry.to_str().unwrap()
        };

        queue!(
            w,
            style::ResetColor,
            terminal::Clear(ClearType::All),
            cursor::Hide,
            cursor::MoveTo(0, 0),
        )?;

        queue!(
            w,
            style::SetForegroundColor(Color::DarkGreen),
            style::SetAttribute(Attribute::Bold),
            style::Print(format!("{}@{}", user_name, host_name)),
            style::SetForegroundColor(Color::White),
            style::Print(":"),
            style::SetForegroundColor(Color::DarkBlue),
            style::Print(format!("{}/", current_dir_display)),
            style::SetForegroundColor(Color::White),
            style::Print(file_stem),
            cursor::MoveToNextLine(1),
        )?;

        let (width, height) = terminal::size()?;
        let second_column = width / 6 + 1;

        // TODO(Chris): Correctly display previous directory column, especially
        // as it relates to the current path.

        // TODO(Chris): Update entries_index
        queue_entries_column(&mut w, 1, width / 6 - 2, height, &prev_entries, 0)?;

        queue_entries_column(
            &mut w,
            second_column,
            width / 2 - 2,
            height,
            &entries,
            entry_index,
        )?;

        w.flush()?;

        match read_char()? {
            'q' => break,
            // TODO(Chris): Account for possibility of no .parent() AKA when
            // current_dir is '/'
            'h' => {
                std::env::set_current_dir("..")?;
                current_dir = current_dir.parent().unwrap().to_path_buf();
                entries = get_sorted_entries(&current_dir);

                prev_dir = current_dir.parent().unwrap().to_path_buf();
                prev_entries = get_sorted_entries(&prev_dir);
            }
            // TODO(Chris): Implement scrolling down to see more entries in large directories
            'j' => {
                if entries.len() > 0
                    && (entry_index as usize) < entries.len() - 1
                    && entry_index + 2 < height
                {
                    entry_index += 1
                }
            }
            'k' => {
                if entry_index > 0 {
                    entry_index -= 1
                }
            }
            _ => (),
        }
    }

    execute!(
        w,
        style::ResetColor,
        cursor::Show,
    )?;


    Ok(())
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
fn cmp_dir_entry(entry1: &DirEntry, entry2: &DirEntry) -> Ordering {
    // FIXME(Chris): Check if you can replace the calls to std::fs::metadata with DirEntry.metadata
    // calls
    let file_type1 = match std::fs::metadata(entry1.path()) {
        Ok(metadata) => metadata.file_type(),
        Err(err) => {
            match err.kind() {
                // Just use name of symbolic link
                io::ErrorKind::NotFound => entry1.metadata().unwrap().file_type(),
                _ => panic!(err),
            }
        },
    };
    let file_type2 = match std::fs::metadata(entry1.path()) {
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
        return cmp_natural(entry1.file_name().to_str().unwrap(), entry2.file_name().to_str().unwrap());
    }
}

fn queue_entries_column(
    w: &mut io::Stdout,
    left_x: u16,
    right_x: u16,
    bottom_y: u16,
    entries: &Vec<DirEntry>,
    entry_index: u16,
) -> crossterm::Result<()> {
    let mut curr_y = 1;

    queue!(
        w,
        style::SetForegroundColor(Color::White),
        style::SetAttribute(Attribute::Reset),
    )?;
    if entries.len() <= 0 {
        queue!(
            w,
            cursor::MoveTo(left_x, curr_y),
            style::Print(" "),
            style::SetAttribute(Attribute::Reverse),
            style::Print("empty"),
            style::SetAttribute(Attribute::Reset),
            style::Print(" "),
        )?;
    } else {
        for entry in entries {
            if curr_y >= bottom_y {
                break;
            }

            let is_curr_entry = curr_y - 1 == entry_index;
            let file_type = std::fs::symlink_metadata(entry.path())?.file_type();

            if is_curr_entry {
                queue!(w, style::SetAttribute(Attribute::Reverse))?;
            }

            if file_type.is_dir() {
                queue!(
                    w,
                    style::SetForegroundColor(Color::DarkBlue),
                    style::SetAttribute(Attribute::Bold)
                )?;
            }

            queue!(w, cursor::MoveTo(left_x, curr_y), style::Print(' '))?;

            let file_name = entry.file_name();
            let file_name = file_name.to_str().unwrap();

            for (index, ch) in file_name.char_indices() {
                if (left_x as usize) + index >= (right_x as usize) - 2 {
                    queue!(w, style::Print('~'),)?;
                    break;
                }

                queue!(w, style::Print(ch),)?;
            }

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

            queue!(w, style::ResetColor)?;

            if is_curr_entry {
                queue!(w, style::SetAttribute(Attribute::Reset))?;
            }

            curr_y += 1;
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
    }
}
