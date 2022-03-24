// FIXME(Chris): Reenable dead code warning
pub mod grid;

use io::Stdout;

use crossterm::{cursor, queue, style, terminal::{self, EnterAlternateScreen, LeaveAlternateScreen}, execute};
use std::io::{self, Write};

pub struct Screen<T>
where
    T: Write,
{
    output: T,
    grid: Grid<Cell>,
    prev_grid: Grid<Cell>,
    cursor_display: (u16, u16),
}

impl<T> Screen<T>
where
    T: Write,
{
    pub fn new(screen_output: T) -> io::Result<Self> {
        let (width, height) = terminal::size()?;

        Ok(Self {
            output: screen_output,
            grid: Grid::new(width, height),
            prev_grid: Grid::new(width, height),
            cursor_display: (0, 0),
        })
    }

    pub fn show_cursor(&mut self, x: u16, y: u16) {
        self.cursor_display = (x, y);
    }

    pub fn set_cell(&mut self, x: u16, y: u16, ch: char) {
        let mut cell = self.grid.get_mut(x, y);
        cell.ch = ch;
    }

    pub fn activate(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        execute!(&mut self.output, EnterAlternateScreen)?;

        Ok(())
    }

    pub fn deactivate(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()?;
        execute!(&mut self.output, LeaveAlternateScreen)?;

        Ok(())
    }
}

impl Screen<Stdout> {
    pub fn show(&mut self) -> io::Result<()> {
        let mut stdout_lock = self.output.lock();

        for x in 0..self.grid.width {
            for y in 0..self.grid.height {
                let cell = self.grid.get(x, y);
                let prev_cell = self.prev_grid.get(x, y);

                if cell != prev_cell {
                    queue!(
                        &mut stdout_lock,
                        cursor::MoveTo(x, y),
                        style::Print(cell.ch)
                    )?;
                }

                // Update the previous buffer

                let prev_cell = self.prev_grid.get_mut(x, y);
                // NOTE(Chris): As long as Cell doesn't do any heap allocations, using clone_from()
                // should allow us to avoid making new heap allocations.
                prev_cell.clone_from(cell);
            }
        }

        queue!(
            &mut stdout_lock,
            cursor::MoveTo(self.cursor_display.0, self.cursor_display.1),
        )?;

        stdout_lock.flush()?;

        Ok(())
    }
}

/// Grid implements a two-dimensional array with a single contiguous buffer
#[derive(Clone)]
pub struct Grid<T> {
    width: u16,
    height: u16,
    buffer: Vec<T>,
}

impl<T> Grid<T>
where
    T: Default,
    T: Clone,
{
    fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            buffer: vec![T::default(); (width * height).into()],
        }
    }
}

impl<T> Grid<T> {
    fn get(&self, x: u16, y: u16) -> &T {
        &self.buffer[coords_to_index(self.width, x, y)]
    }

    // This attribute and profile will disable compilation of this function unless testing,
    // eliminating the dead code error for this function.
    #[cfg(test)]
    fn set(&mut self, x: u16, y: u16, value: T) {
        self.buffer[coords_to_index(self.width, x, y)] = value;
    }

    fn get_mut(&mut self, x: u16, y: u16) -> &mut T {
        &mut self.buffer[coords_to_index(self.width, x, y)]
    }
}

fn coords_to_index(width: u16, x: u16, y: u16) -> usize {
    (y * width + x).into()
}

#[derive(Clone, Copy, Default, PartialEq)]
pub struct Cell {
    ch: char,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_get() {
        let mut grid = Grid::new(4, 5);

        grid.set(2, 3, 'a');
        grid.set(3, 4, 'Z');

        assert_eq!(grid.get(2, 3), &'a');
    }
}
