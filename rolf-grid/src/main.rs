use std::io;

use crossterm::event::{self, Event, KeyCode};
use rolf_grid::{Attribute, Screen, Style, Color};

fn main() -> io::Result<()> {
    let mut screen = Screen::new(io::stdout())?;

    screen.activate()?;

    let mut x = 1;
    let mut y = 1;

    let mut is_cursor_visible = true;

    let (width, height) = screen.size();

    let items = vec![
        "10.bak",
        "11.bak",
        "12.bak",
        "13.bak",
        "14.bak",
        "15.bak",
        "16.bak",
        "17.bak",
        "18.bak",
        "1.bak",
        "2.bak",
        "3.bak",
        "4.bak",
        "5.bak",
        "6.bak",
        "7.bak",
        "8.bak",
        "9.bak",
        "basic.svg",
        "Cargo.lock",
        "Cargo.toml",
        "ci",
        "demo.gif",
        "flamegraph.svg",
        "LICENSE",
        "link_dir",
        "link_invalid",
        "link_txt",
        "none",
        "perf.data",
        "perf.data.old",
        "README.md",
        "rolf-grid",
        "rolf-parser",
        "rolfrc",
        "rustfmt.toml",
        "src",
        "ssl",
        "stderr.txt",
        "target",
    ];

    let mut file_top_ind = 0;
    let mut file_curr_ind = 10;

    loop {
        screen.clear_logical();

        screen.set_cell(0, 0, '┌');
        screen.set_cell(width - 1, 0, '┐');
        screen.set_cell(0, height - 1, '└');
        screen.set_cell(width - 1, height - 1, '┘');

        for y in 1..height - 1 {
            screen.set_cell(0, y, '│');
            screen.set_cell(width - 1, y, '│');
        }

        for x in 1..width - 1 {
            screen.set_cell(x, 0, '─');
            screen.set_cell(x, height - 1, '─');
        }

        if is_cursor_visible {
            screen.show_cursor(x, y);
        } else {
            screen.hide_cursor();
        }
        draw_str(
            &mut screen,
            10,
            1,
            "Welcome!",
            Style::new_attr(Attribute::Bold)
        );
        draw_str(
            &mut screen,
            10,
            3,
            "This is underlined.",
            Style::new_attr(Attribute::Underlined),
        );
        draw_str(
            &mut screen,
            10,
            5,
            "This is underlined and bold.",
            Style::new_attr(Attribute::Underlined | Attribute::Bold),
        );
        screen.set_cell(x, y, '@');

        for y in 1..=height - 2 {
            let ind = file_top_ind + y - 1;

            let mut draw_style = if ind == file_curr_ind {
                Style::new_attr(Attribute::Reverse)
            } else {
                Style::new_attr(Attribute::None)
            };

            if ind < 5 {
                draw_style.fg = Color::Blue;
            }

            let file_name = if (ind as usize) < items.len() {
                items[ind as usize]
            } else {
                ""
            };

            draw_str(&mut screen, 50, y, file_name, draw_style);
        }

        screen.show()?;

        let event = event::read()?;

        match event {
            Event::Key(key_event) => match key_event.code {
                KeyCode::Char('j') => {
                    if y < height - 1 {
                        y += 1;
                    }
                }
                KeyCode::Char('k') => {
                    if y > 0 {
                        y -= 1;
                    }
                }
                KeyCode::Char('l') => {
                    if x < width - 1 {
                        x += 1;
                    }
                }
                KeyCode::Char('h') => {
                    if x > 0 {
                        x -= 1;
                    }
                }
                KeyCode::Char('J') => {
                    if (file_curr_ind as usize) < items.len() - 1 {
                        file_curr_ind += 1;
                    }
                }
                KeyCode::Char('K') => {
                    if file_curr_ind > 0 {
                        file_curr_ind -= 1;
                    }
                }
                KeyCode::Char('s') => {
                    is_cursor_visible = !is_cursor_visible;
                }
                KeyCode::Char('+') => {
                    file_top_ind += 1;
                }
                KeyCode::Char('-') => {
                    if file_top_ind > 0 {
                        file_top_ind -= 1;
                    }
                }
                KeyCode::Char('q') => break,
                _ => (),
            },
            Event::Mouse(..) => (),
            Event::Resize(_, _) => (),
        }
    }

    screen.deactivate()?;

    Ok(())
}

fn draw_str(screen: &mut Screen<io::Stdout>, x: u16, y: u16, string: &str, style: Style) {
    for (i, ch) in string.char_indices() {
        let i: u16 = i.try_into().expect("Should be able to fit into a u16.");
        screen.set_cell_style(x + i, y, ch, style);
    }
}
