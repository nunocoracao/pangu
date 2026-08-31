//! Full-screen loading animation with Matrix rain and pulsing logo

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

/// Matrix rain character set - katakana (double-width) for authentic Matrix look
const MATRIX_CHARS: &[char] = &[
    'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ', 'ク', 'ケ', 'コ',
    'サ', 'シ', 'ス', 'セ', 'ソ', 'タ', 'チ', 'ツ', 'テ', 'ト',
    'ナ', 'ニ', 'ヌ', 'ネ', 'ノ', 'ハ', 'ヒ', 'フ', 'ヘ', 'ホ',
    'マ', 'ミ', 'ム', 'メ', 'モ', 'ヤ', 'ユ', 'ヨ', 'ラ', 'リ',
    'ル', 'レ', 'ロ', 'ワ', 'ヲ', 'ン',
];

/// Character width for matrix rain (katakana = 2 cells)
const MATRIX_CHAR_WIDTH: u16 = 2;

/// Pangu ASCII art logo
const LOGO: &[&str] = &[
    "██████╗  █████╗ ███╗   ██╗ ██████╗ ██╗   ██╗",
    "██╔══██╗██╔══██╗████╗  ██║██╔════╝ ██║   ██║",
    "██████╔╝███████║██╔██╗ ██║██║  ███╗██║   ██║",
    "██╔═══╝ ██╔══██║██║╚██╗██║██║   ██║██║   ██║",
    "██║     ██║  ██║██║ ╚████║╚██████╔╝╚██████╔╝",
    "╚═╝     ╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝  ╚═════╝ ",
];

/// Progress bar width in characters
const PROGRESS_BAR_WIDTH: usize = 40;
/// Tick budget for a full logo color cycle (slower = calmer)
const LOGO_COLOR_CYCLE_TICKS: u64 = 300;
/// How often to rotate loading/download fun messages
const MESSAGE_CYCLE_TICKS: u64 = 120;
/// Typewriter pacing (ticks per character)
const TYPEWRITER_TICKS_PER_CHAR: u64 = 3;
/// Spinner animation speed divisor
const SPINNER_TICK_DIVISOR: u64 = 4;

/// Fun sentences to display during model loading (rotates every few seconds)
const LOADING_MESSAGES: &[&str] = &[
    "Waking up the neurons...",
    "Stretching the tensors...",
    "Warming up the GPU...",
    "Assembling the transformer...",
    "Connecting synapses...",
    "Booting consciousness...",
    "Loading imagination module...",
    "Initializing creativity...",
    "Spinning up attention heads...",
    "Preparing to think...",
    "Calibrating intelligence...",
    "Unfreezing the weights...",
    "Hydrating the model...",
    "Plugging in the brain...",
    "Starting the think tank...",
    "Revving neural engines...",
    "Activating language cores...",
    "Loading common sense...",
    "Brewing fresh tokens...",
    "Tuning the frequencies...",
    "Aligning the matrices...",
    "Charging creative batteries...",
    "Summoning the AI...",
    "Preparing witty responses...",
    "Loading sarcasm module...",
];

/// Fun sentences to display during download (rotates every few seconds)
const DOWNLOAD_MESSAGES: &[&str] = &[
    "Teaching silicon to think...",
    "Downloading digital wisdom...",
    "Acquiring artificial neurons...",
    "Fetching the matrix...",
    "Loading neural pathways...",
    "Summoning the AI spirits...",
    "Compressing human knowledge...",
    "Calibrating quantum thoughts...",
    "Installing digital brain cells...",
    "Downloading creative juice...",
    "Brewing artificial coffee...",
    "Defragmenting consciousness...",
    "Updating reality drivers...",
    "Syncing with the hive mind...",
    "Charging flux capacitors...",
    "Warming up the thinking cap...",
    "Polishing neural networks...",
    "Untangling weight matrices...",
    "Feeding the attention heads...",
    "Stacking more transformers...",
    "Tokenizing the universe...",
    "Embedding human experience...",
    "Training on cat pictures...",
    "Optimizing gradient descent...",
    "Escaping local minima...",
    "Adjusting hyperparameters...",
    "Greasing the tensor gears...",
    "Inflating language balloons...",
    "Sharpening prediction edges...",
    "Aligning with human values...",
];

/// Loading screen state - determines what to display
#[derive(Debug, Clone)]
pub enum LoadingState {
    /// Downloading the model with progress info
    Downloading {
        downloaded: u64,
        total: u64,
        speed: f64,
    },
    /// Loading the model into memory
    Loading,
}

/// Simple hash function for better randomness
fn hash_mix(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^= x >> 33;
    x
}

/// State for a single matrix rain column
#[derive(Clone)]
pub struct MatrixColumn {
    /// Current y position of the lead character
    pub y: f32,
    /// Fall speed (cells per tick)
    pub speed: f32,
    /// Current character at each position (sparse - only trailing chars)
    pub chars: Vec<char>,
    /// Trail length
    pub trail_len: usize,
    /// Internal seed for this column (for randomness)
    seed: u64,
    /// Mutation rate (some columns mutate faster)
    mutation_rate: u64,
}

impl MatrixColumn {
    /// Create a new column with random initial state
    pub fn new(height: u16, seed: u64) -> Self {
        let h = hash_mix(seed);
        let h2 = hash_mix(h);
        let h3 = hash_mix(h2);

        // More varied speeds: 0.2 to 1.0
        let speed = 0.2 + (h % 1000) as f32 / 1250.0;
        // Start at random positions, some way above screen
        let start_y = -((h2 % (height as u64 * 3 + 10)) as f32);
        // Trail length: 3 to 14
        let trail_len = 3 + (h3 % 12) as usize;
        // Mutation rate: some columns change chars more frequently
        let mutation_rate = 2 + (hash_mix(h3) % 5);

        Self {
            y: start_y,
            speed,
            chars: Vec::with_capacity(trail_len),
            trail_len,
            seed: h,
            mutation_rate,
        }
    }

    /// Update the column position and generate new characters
    pub fn update(&mut self, height: u16, tick: u64, col_idx: usize) {
        self.y += self.speed;

        // Reset if we've gone past the screen
        if self.y > height as f32 + self.trail_len as f32 {
            // Mix in tick for varied reset timing
            let reset_hash = hash_mix(tick.wrapping_add(self.seed).wrapping_mul(col_idx as u64 + 1));
            // Random delay before reappearing (5 to 35 cells above)
            self.y = -(self.trail_len as f32) - (5.0 + (reset_hash % 30) as f32);
            // Slightly vary speed on reset
            let speed_hash = hash_mix(reset_hash);
            self.speed = 0.2 + (speed_hash % 1000) as f32 / 1250.0;
            // Possibly change trail length
            if reset_hash % 4 == 0 {
                self.trail_len = 3 + (hash_mix(speed_hash) % 12) as usize;
            }
            self.chars.clear();
            self.seed = reset_hash;
        }

        // Generate characters for the trail using varied seeds
        while self.chars.len() < self.trail_len {
            let char_hash = hash_mix(self.seed.wrapping_add(self.chars.len() as u64 * 7919));
            let idx = (char_hash as usize) % MATRIX_CHARS.len();
            self.chars.push(MATRIX_CHARS[idx]);
        }

        // Randomly change some characters for the "mutation" effect
        // Different columns mutate at different rates
        if tick % self.mutation_rate == 0 && !self.chars.is_empty() {
            let mutation_hash = hash_mix(tick.wrapping_mul(self.seed));
            let idx = (mutation_hash as usize) % self.chars.len();
            let char_idx = (hash_mix(mutation_hash) as usize) % MATRIX_CHARS.len();
            self.chars[idx] = MATRIX_CHARS[char_idx];

            // Sometimes mutate a second character
            if mutation_hash % 3 == 0 && self.chars.len() > 1 {
                let idx2 = (hash_mix(mutation_hash + 1) as usize) % self.chars.len();
                let char_idx2 = (hash_mix(mutation_hash + 2) as usize) % MATRIX_CHARS.len();
                self.chars[idx2] = MATRIX_CHARS[char_idx2];
            }
        }
    }
}

/// Full-screen loading animation
pub struct LoadingScreen<'a> {
    tick: u64,
    matrix_columns: &'a mut Vec<MatrixColumn>,
    state: LoadingState,
}

impl<'a> LoadingScreen<'a> {
    pub fn new(tick: u64, matrix_columns: &'a mut Vec<MatrixColumn>, state: LoadingState) -> Self {
        Self { tick, matrix_columns, state }
    }

    /// Get the pulsing color for the logo based on tick
    fn logo_color(&self) -> Color {
        // Cycle through colors: Cyan -> Blue -> Magenta -> Cyan
        // Full cycle every LOGO_COLOR_CYCLE_TICKS.
        let phase = (self.tick % LOGO_COLOR_CYCLE_TICKS) as f32 / LOGO_COLOR_CYCLE_TICKS as f32;

        if phase < 0.33 {
            // Cyan to Blue
            let t = phase / 0.33;
            interpolate_color(Color::Cyan, Color::Blue, t)
        } else if phase < 0.66 {
            // Blue to Magenta
            let t = (phase - 0.33) / 0.33;
            interpolate_color(Color::Blue, Color::Magenta, t)
        } else {
            // Magenta to Cyan
            let t = (phase - 0.66) / 0.34;
            interpolate_color(Color::Magenta, Color::Cyan, t)
        }
    }

    /// Render matrix rain to the buffer
    fn render_matrix(&mut self, buf: &mut Buffer, area: Rect) {
        // Number of columns accounting for double-width characters
        let num_columns = (area.width / MATRIX_CHAR_WIDTH) as usize;
        let height = area.height;

        // Initialize columns if needed (one per double-width character position)
        if self.matrix_columns.len() != num_columns {
            self.matrix_columns.clear();
            for i in 0..num_columns {
                let seed = (i as u64).wrapping_mul(12345).wrapping_add(self.tick);
                self.matrix_columns.push(MatrixColumn::new(height, seed));
            }
        }

        // Update and render each column
        for (col_idx, column) in self.matrix_columns.iter_mut().enumerate() {
            column.update(height, self.tick, col_idx);

            let lead_y = column.y as i32;

            // Render the trail
            for (i, &ch) in column.chars.iter().enumerate() {
                let y = lead_y - i as i32;
                if y >= 0 && y < height as i32 {
                    // Position at every MATRIX_CHAR_WIDTH cells (0, 2, 4, ...)
                    let x = area.x + (col_idx as u16 * MATRIX_CHAR_WIDTH);
                    let cell_y = area.y + y as u16;

                    if x + MATRIX_CHAR_WIDTH <= area.x + area.width && cell_y < area.y + area.height {
                        // Color fades from bright to dark green
                        let brightness = if i == 0 {
                            255 // Lead character is brightest
                        } else {
                            let fade = 1.0 - (i as f32 / column.trail_len as f32);
                            (fade * 180.0) as u8 + 40
                        };

                        let color = Color::Rgb(0, brightness, 0);
                        if let Some(cell) = buf.cell_mut((x, cell_y)) {
                            cell.set_char(ch).set_style(Style::default().fg(color));
                        }
                    }
                }
            }
        }
    }

    /// Render the centered logo with pulsing color
    fn render_logo(&self, buf: &mut Buffer, area: Rect) {
        let logo_width = LOGO[0].chars().count() as u16;
        let logo_height = LOGO.len() as u16;

        // Calculate extra height needed based on state
        let extra_height: u16 = match &self.state {
            LoadingState::Downloading { .. } => 10, // Title + fun message + progress + size + speed
            LoadingState::Loading => 6, // Spinner + fun message
        };

        // Calculate box dimensions first - use the wider of logo or progress bar
        let box_padding = 3u16;
        let content_width = logo_width.max(PROGRESS_BAR_WIDTH as u16 + 8);
        let box_width = content_width + box_padding * 2;
        let box_height = logo_height + extra_height + 2;

        // Center the box in the area
        let box_x = area.x + area.width.saturating_sub(box_width) / 2;
        let box_y = area.y + area.height.saturating_sub(box_height) / 2;

        // Logo position (centered within the box)
        let logo_x = box_x + (box_width.saturating_sub(logo_width)) / 2;
        let logo_y = box_y + 1;

        let color = self.logo_color();

        // Draw a dark background box behind the logo for readability
        // Must properly clear double-width characters from matrix rain
        let box_bg = Style::default().bg(Color::Rgb(10, 10, 20));
        for y in box_y..box_y + box_height {
            for x in box_x..box_x + box_width {
                if x < area.x + area.width && y < area.y + area.height && x >= area.x && y >= area.y {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        // Reset the cell completely before setting new content
                        // This ensures double-width character remnants are cleared
                        cell.reset();
                        cell.set_char(' ').set_style(box_bg);
                    }
                }
            }
        }

        // Draw the logo
        for (i, line) in LOGO.iter().enumerate() {
            let y = logo_y + i as u16;
            if y >= area.y && y < area.y + area.height {
                for (j, ch) in line.chars().enumerate() {
                    let x = logo_x + j as u16;
                    if x >= area.x && x < area.x + area.width {
                        if let Some(cell) = buf.cell_mut((x, y)) {
                            cell.set_char(ch).set_style(Style::default().fg(color).bg(Color::Rgb(10, 10, 20)));
                        }
                    }
                }
            }
        }

        // Draw state-specific content below the logo
        let content_y = logo_y + logo_height + 2;
        match &self.state {
            LoadingState::Downloading { downloaded, total, speed } => {
                self.render_download_progress(buf, area, box_x, box_width, content_y, *downloaded, *total, *speed);
            }
            LoadingState::Loading => {
                self.render_loading_spinner(buf, area, box_x, box_width, content_y);
            }
        }
    }

    /// Render the loading spinner with fun rotating messages
    fn render_loading_spinner(&self, buf: &mut Buffer, area: Rect, box_x: u16, box_width: u16, y: u16) {
        let bg = Style::default().bg(Color::Rgb(10, 10, 20));

        // Helper to render a line of text centered in the box
        let render_line = |buf: &mut Buffer, line_y: u16, text: &str, style: Style| {
            if line_y < area.y || line_y >= area.y + area.height {
                return;
            }
            let text_len = text.len() as u16;
            let start_x = box_x + (box_width.saturating_sub(text_len)) / 2;

            for (i, ch) in text.bytes().enumerate() {
                let x = start_x + i as u16;
                if x >= area.x && x < area.x + area.width {
                    if let Some(cell) = buf.cell_mut((x, line_y)) {
                        cell.set_char(ch as char).set_style(style);
                    }
                }
            }
        };

        // Line 1: Spinner with "Loading model..."
        let ascii_spinner = ["|", "/", "-", "\\"];
        let spinner_frame = ascii_spinner[((self.tick / SPINNER_TICK_DIVISOR) as usize) % ascii_spinner.len()];
        let loading_text = format!("{} Loading model...", spinner_frame);
        render_line(buf, y, &loading_text, bg.fg(Color::Yellow));

        // Line 2: Fun rotating message with typewriter effect
        // Message changes every MESSAGE_CYCLE_TICKS.
        let message_cycle = MESSAGE_CYCLE_TICKS;
        let message_idx = (self.tick / message_cycle) as usize % LOADING_MESSAGES.len();
        let fun_message = LOADING_MESSAGES[message_idx];

        // Calculate how many characters to show (typewriter effect)
        let ticks_into_message = self.tick % message_cycle;
        let chars_to_show = (ticks_into_message / TYPEWRITER_TICKS_PER_CHAR) as usize;
        let chars_to_show = chars_to_show.min(fun_message.len());

        // Render the partial message with a cursor
        let visible_message: String = if chars_to_show < fun_message.len() {
            format!("{}|", &fun_message[..chars_to_show])
        } else {
            fun_message.to_string()
        };

        // Fade color based on how complete the message is
        let brightness = if chars_to_show >= fun_message.len() {
            200u8 // Full brightness when complete
        } else {
            150u8 // Slightly dimmer while typing
        };
        let msg_color = Color::Rgb(100, brightness, brightness); // Cyan-ish (different from download)

        render_line(buf, y + 1, &visible_message, bg.fg(msg_color));
    }

    /// Render the download progress bar and stats
    fn render_download_progress(&self, buf: &mut Buffer, area: Rect, box_x: u16, box_width: u16, start_y: u16, downloaded: u64, total: u64, speed: f64) {
        let bg = Style::default().bg(Color::Rgb(10, 10, 20));

        // Helper to render a line of text centered in the box
        let render_line = |buf: &mut Buffer, y: u16, text: &str, style: Style| {
            if y < area.y || y >= area.y + area.height {
                return;
            }
            let text_len = text.len() as u16;
            let start_x = box_x + (box_width.saturating_sub(text_len)) / 2;

            for (i, ch) in text.bytes().enumerate() {
                let x = start_x + i as u16;
                if x >= area.x && x < area.x + area.width {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_char(ch as char).set_style(style);
                    }
                }
            }
        };

        // Line 1: Main title (always visible)
        render_line(buf, start_y, "Downloading model...", bg.fg(Color::Cyan));

        // Line 2: Fun rotating message with typewriter effect
        // Message changes every MESSAGE_CYCLE_TICKS.
        let message_cycle = MESSAGE_CYCLE_TICKS;
        let message_idx = (self.tick / message_cycle) as usize % DOWNLOAD_MESSAGES.len();
        let fun_message = DOWNLOAD_MESSAGES[message_idx];

        // Calculate how many characters to show (typewriter effect)
        let ticks_into_message = self.tick % message_cycle;
        let chars_to_show = (ticks_into_message / TYPEWRITER_TICKS_PER_CHAR) as usize;
        let chars_to_show = chars_to_show.min(fun_message.len());

        // Render the partial message with a cursor
        let visible_message: String = if chars_to_show < fun_message.len() {
            format!("{}|", &fun_message[..chars_to_show])
        } else {
            fun_message.to_string()
        };

        // Fade color based on how complete the message is
        let brightness = if chars_to_show >= fun_message.len() {
            200u8 // Full brightness when complete
        } else {
            150u8 // Slightly dimmer while typing
        };
        let msg_color = Color::Rgb(brightness, 100, brightness); // Magenta-ish

        render_line(buf, start_y + 1, &visible_message, bg.fg(msg_color));

        // Line 3: Progress bar (using ASCII characters for reliability)
        let progress_y = start_y + 3;
        let percentage = if total > 0 { downloaded as f64 / total as f64 } else { 0.0 };
        let filled = (percentage * PROGRESS_BAR_WIDTH as f64) as usize;
        let empty = PROGRESS_BAR_WIDTH - filled;

        // Use ASCII = and - for progress bar (reliable across all terminals)
        let progress_bar = format!(
            "[{}{}] {:3.0}%",
            "=".repeat(filled),
            "-".repeat(empty),
            percentage * 100.0
        );

        // Render progress bar with colors
        if progress_y >= area.y && progress_y < area.y + area.height {
            let text_len = progress_bar.len() as u16;
            let start_x = box_x + (box_width.saturating_sub(text_len)) / 2;

            for (i, ch) in progress_bar.bytes().enumerate() {
                let x = start_x + i as u16;
                if x >= area.x && x < area.x + area.width {
                    let color = if ch == b'=' { Color::Green } else { Color::DarkGray };
                    if let Some(cell) = buf.cell_mut((x, progress_y)) {
                        cell.set_char(ch as char).set_style(bg.fg(color));
                    }
                }
            }
        }

        // Line 4: Size info
        let size_text = format!("{} / {}", format_bytes(downloaded), format_bytes(total));
        render_line(buf, start_y + 5, &size_text, bg.fg(Color::White));

        // Line 5: Speed info
        let speed_text = format!("{}/s", format_bytes(speed as u64));
        render_line(buf, start_y + 6, &speed_text, bg.fg(Color::Yellow));
    }
}

impl Widget for LoadingScreen<'_> {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        // Clear with black background
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(' ').set_style(Style::default().bg(Color::Black));
                }
            }
        }

        // Render matrix rain first (background)
        self.render_matrix(buf, area);

        // Render logo on top (with dark box behind it)
        self.render_logo(buf, area);
    }
}

/// Interpolate between two colors
fn interpolate_color(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);

    let (r1, g1, b1) = color_to_rgb(from);
    let (r2, g2, b2) = color_to_rgb(to);

    let r = (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8;
    let g = (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8;
    let b = (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8;

    Color::Rgb(r, g, b)
}

/// Convert a Color to RGB values
fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Cyan => (0, 255, 255),
        Color::Blue => (0, 100, 255),
        Color::Magenta => (255, 0, 255),
        Color::Green => (0, 255, 0),
        Color::Yellow => (255, 255, 0),
        _ => (255, 255, 255),
    }
}

/// Format bytes as human-readable string (e.g., "1.5 GB")
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
