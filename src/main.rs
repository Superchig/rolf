use std::cmp::Ordering;
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
    Result,
};

fn main() -> Result<()> {
    let mut w = io::stdout();

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

    execute!(w, terminal::EnterAlternateScreen)?;

    terminal::enable_raw_mode()?;

    // NOTE(Chris): The default column ratio is 1:2:3

    let mut entry_index = 0;

    // TODO(Chris): Eliminate a bunch of unwrap() calls by actually handling errors

    let mut current_dir = std::env::current_dir()?;

    let mut entries = get_sorted_entries(&current_dir);

    // Main input loop
    loop {
        // TODO(Chris): Handle case when current_dir is '/'
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
            // TODO(Chris): Figure out how to use simply a str type both here and in its
            // the corresponding if clause, rather than creating a new String (and thus
            // doing a heap allocation).
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

        queue_entries_column(&mut w, 1, width / 6 - 2, height, &entries, entry_index)?;

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
        terminal::LeaveAlternateScreen,
    )?;

    w.flush()?;

    terminal::disable_raw_mode()?;

    println!("Goodbye.");

    Ok(())
}

fn read_char() -> Result<char> {
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
    let file_type1 = std::fs::metadata(entry1.path()).unwrap().file_type();
    let file_type2 = std::fs::metadata(entry2.path()).unwrap().file_type();

    if file_type1.is_dir() && file_type2.is_file() {
        return Ordering::Less;
    } else if file_type2.is_dir() && file_type1.is_file() {
        return Ordering::Greater;
    } else {
        return cmp_alphanum(entry1.file_name().to_str().unwrap(), entry2.file_name().to_str().unwrap());
    }
}

fn cmp_alphanum(str1: &str, str2: &str) -> Ordering {
    cmp_alphanum_2(str1, str2)
}

// NOTE(Chris): This is adapted from lf's natural less implementation, which can be found in its
// misc.go file.
// https://github.com/gokcehan/lf/blob/55b9189713f40b5d2058fad7cf77f82d902485f1/misc.go#L173
// NOTE(Chris): lf's algorithm uses the lo1, lo2, hi1, and hi2 variables to keep track of the
// "chunks" in each string, comparing them as necessary. By using these index variables, this
// algorithm doesn't seem to make any heap allocations. Unfortunately, my implementation of the
// algorithm does.
fn natural_less(str1: &str, str2: &str) -> bool {
    // NOTE(Chris): This is going to involve some more
    // allocations than may be strictly necessary, but
    // we're doing this so we can easily index chars1
    // and chars2.
    let s1: Vec<char> = str1.chars().collect();
    let s2: Vec<char> = str2.chars().collect();

    let mut lo1: usize;
    let mut lo2: usize;
    let mut hi1 = 0;
    let mut hi2 = 0;

    loop {
        // Return true if s1 has run out of characters, but s2 still has characters left.  If s2
        // has also run out of characters, then s1 and s2 are equal (or so I would think), in which
        // case return false.
        if hi1 >= s1.len() {
            return hi2 != s2.len();
        }

        // Since the previous if block didn't return, s1 has not run out of characters and yet s2
        // has. So, s2 is a prefix of s1 and really s1 is greater than s2, so return false.
        if hi2 >= s2.len() {
            return false;
        }

        let is_digit_1 = s1[hi1].is_numeric();
        let is_digit_2 = s2[hi2].is_numeric();

        // This advances lo1 and hi1 to the next chunk, with hi1 being the exclusive last index of
        // the chunk.
        lo1 = hi1;
        while hi1 < s1.len() && s1[hi1].is_numeric() == is_digit_1 {
            hi1 += 1;
        }

        // This advances lo2 and hi2 to the next chunk, with hi2 being the exclusive last index of
        // the chunk.
        lo2 = hi2;
        while hi2 < s2.len() && s2[hi2].is_numeric() == is_digit_2 {
            hi2 += 1;
        }

        // If the string forms of the chunks are equal, then keep going. We haven't found out the
        // ordering of the overall strings yet.
        if s1[lo1..hi1] == s2[lo2..hi2] {
            continue
        }

        // If both chunks are digits, then convert them into actual ints and compare them
        if is_digit_1 && is_digit_2 {
            // TODO(Chris): Avoid allocating new Strings here
            let s1: String = s1[lo1..hi1].into_iter().collect();
            let s2: String = s2[lo2..hi2].into_iter().collect();
            if let (Ok(num1), Ok(num2)) = (s1.parse::<usize>(), s2.parse::<usize>()) {
                return num1 < num2;
            }
        }

        // If we've made it this far, then neither the string forms of the chunks are equal nor are
        // both of the chunks actually numerical. Thus, these chunks are the ones which will
        // finally determine if the order of the strings, so we only need to compare them.
        return s1[lo1..hi1] < s2[lo2..hi2];
    }
}

fn cmp_alphanum_2(str1: &str, str2: &str) -> Ordering {
    if natural_less(str1, str2) {
        return Ordering::Less;
    } else if str1 == str2 {
        return Ordering::Equal;
    } else {
        return Ordering::Greater;
    }
}

fn queue_entries_column(
    w: &mut io::Stdout,
    left_x: u16,
    right_x: u16,
    bottom_y: u16,
    entries: &Vec<DirEntry>,
    entry_index: u16,
) -> Result<()> {
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

// TODO(Chris): Change the string sorting to sort numerical values differently
// i.e. 1, 10, 2 -> 1, 2, 10
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
        assert_eq!(cmp_alphanum("10.bak", "1.bak"), Ordering::Greater);
        assert_eq!(cmp_alphanum("1.bak", "10.bak"), Ordering::Less);

        assert_eq!(cmp_alphanum("2.bak", "10.bak"), Ordering::Less);

        assert_eq!(cmp_alphanum("1.bak", "Cargo.lock"), Ordering::Less);

        assert_eq!(cmp_alphanum(".gitignore", "src"), Ordering::Less);

        assert_eq!(cmp_alphanum(".gitignore", ".gitignore"), Ordering::Equal);
    }

    #[test]
    fn scratch() {
        println!("{:#?}", "C".cmp("1"));
        assert!(true);
    }
}
