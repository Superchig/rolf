use std::io;

use crossterm::event::{self, Event, KeyCode};
use rolf_grid::Screen;

fn main() -> io::Result<()> {
    let mut screen = Screen::new(io::stdout())?;

    screen.activate()?;

    let mut x = 10;
    let mut y = 10;

    let (width, height) = screen.size();

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

    loop {
        screen.show_cursor(x, y);
        screen.set_cell(x, y, '@');
        screen.show()?;

        screen.set_cell(x, y, ' ');

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
