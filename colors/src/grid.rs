//! An image split into equal cells, each filled by one item in layout order: how gradient and
//! palette images are laid out; see docs/spec/agradient.md#image in the assetc repository.

use crate::Image;

/// The largest width and height of a grid, px.
pub const MAX_SIZE: u32 = 4096;

/// The order items fill the grid's cells in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Layout {
    /// Left to right, then top to bottom.
    #[default]
    Rows,
    /// Top to bottom, then left to right.
    Columns,
}

impl Layout {
    pub const ALL: [Self; 2] = [Self::Rows, Self::Columns];

    /// Its key in files.
    pub fn key(self) -> &'static str {
        match self {
            Self::Rows => "rows",
            Self::Columns => "columns",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Rows => "Rows",
            Self::Columns => "Columns",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Rows => "Left to right, then down",
            Self::Columns => "Top to bottom, then right",
        }
    }
}

/// `Ok` when `value` is within `min..=max`, else why not, naming `key`.
pub fn check(key: &str, value: i64, min: i64, max: i64) -> Result<(), String> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!("{key} {value} is outside {min}–{max}"))
    }
}

/// Something laid out as a grid of cells: implement the first four methods.
pub trait CellGrid {
    /// Width, height, cell width, cell height, px.
    fn grid(&self) -> [u32; 4];
    fn layout(&self) -> Layout;
    /// How many cells are filled.
    fn used(&self) -> usize;
    /// What fills the cells, e.g. `gradients`, for errors.
    fn noun(&self) -> &'static str;

    fn columns(&self) -> u32 {
        let [width, _, cell_width, _] = self.grid();
        width / cell_width.max(1)
    }

    fn rows(&self) -> u32 {
        let [_, height, _, cell_height] = self.grid();
        height / cell_height.max(1)
    }

    /// Column and row of item `i`'s cell.
    fn cell(&self, i: usize) -> (u32, u32) {
        let i = i as u32;
        match self.layout() {
            Layout::Rows => (i % self.columns(), i / self.columns()),
            Layout::Columns => (i / self.rows(), i % self.rows()),
        }
    }

    /// How many cells the grid has.
    fn capacity(&self) -> usize {
        (self.columns() * self.rows()) as usize
    }

    /// Checks the sizes (1 to [`MAX_SIZE`], cells within the image) and that the items fit.
    fn validate(&self) -> Result<(), String> {
        let [width, height, cell_width, cell_height] = self.grid();
        let max = i64::from(MAX_SIZE);
        for (key, value) in [("width", width), ("height", height)] {
            check(key, i64::from(value), 1, max)?;
        }
        check("cell_width", i64::from(cell_width), 1, i64::from(width))?;
        check("cell_height", i64::from(cell_height), 1, i64::from(height))?;
        let n = self.used();
        if n > self.capacity() {
            return Err(format!(
                "{n} {} need {n} cells but the grid has {}",
                self.noun(),
                self.capacity()
            ));
        }
        Ok(())
    }

    /// The image: cell `i` drawn by `fill(i, cell_width, cell_height)` (RGBA8 rows), the rest
    /// transparent.
    fn render_cells(&self, fill: impl Fn(usize, u32, u32) -> Vec<u8>) -> Result<Image, String> {
        self.validate()?;
        let [width, height, cell_width, cell_height] = self.grid();
        let mut pixels = vec![0; (width * height * 4) as usize];
        for i in 0..self.used() {
            let (column, row) = self.cell(i);
            let (left, top) = (column * cell_width, row * cell_height);
            let cell = fill(i, cell_width, cell_height);
            let row_bytes = (cell_width * 4) as usize;
            for (y, row) in cell.chunks_exact(row_bytes).enumerate() {
                let start = (((top + y as u32) * width + left) * 4) as usize;
                pixels[start..start + row_bytes].copy_from_slice(row);
            }
        }
        Ok(Image {
            width,
            height,
            pixels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Swatches {
        grid: [u32; 4],
        layout: Layout,
        colors: Vec<[u8; 4]>,
    }

    impl CellGrid for Swatches {
        fn grid(&self) -> [u32; 4] {
            self.grid
        }
        fn layout(&self) -> Layout {
            self.layout
        }
        fn used(&self) -> usize {
            self.colors.len()
        }
        fn noun(&self) -> &'static str {
            "colors"
        }
    }

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * image.width + x) * 4) as usize;
        image.pixels[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn cells_fill_in_layout_order() {
        let mut swatches = Swatches {
            grid: [4, 4, 2, 2],
            layout: Layout::Rows,
            colors: vec![[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]],
        };
        let fill = |s: &Swatches| {
            s.render_cells(|i, w, h| s.colors[i].repeat((w * h) as usize))
                .unwrap()
        };
        let image = fill(&swatches);
        assert_eq!(pixel(&image, 3, 0), [0, 255, 0, 255]);
        assert_eq!(pixel(&image, 0, 3), [0, 0, 255, 255]);
        assert_eq!(pixel(&image, 3, 3), [0; 4]);
        swatches.layout = Layout::Columns;
        let image = fill(&swatches);
        assert_eq!(pixel(&image, 0, 3), [0, 255, 0, 255]);
        assert_eq!(swatches.cell(2), (1, 0));
    }

    #[test]
    fn items_must_fit() {
        let swatches = Swatches {
            grid: [4, 2, 2, 2],
            layout: Layout::Rows,
            colors: vec![[0; 4]; 3],
        };
        assert_eq!(
            swatches.validate().unwrap_err(),
            "3 colors need 3 cells but the grid has 2"
        );
        let wide = Swatches {
            grid: [4, 2, 8, 2],
            ..swatches
        };
        assert_eq!(wide.validate().unwrap_err(), "cell_width 8 is outside 1–4");
    }
}
