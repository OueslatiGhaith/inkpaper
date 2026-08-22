pub const WIDTH: usize = 800;
pub const HEIGHT: usize = 480;

pub const STRIDE: usize = WIDTH / 8;

pub const FRAMEBUFFER_LEN: usize = STRIDE * HEIGHT;

pub fn draw(frame: &mut [u8; FRAMEBUFFER_LEN]) {
    // 1 = white, 0 = black.
    frame.fill(0xff);

    // outer border.
    fill_rect(frame, 0, 0, WIDTH, 8);
    fill_rect(frame, 0, HEIGHT - 8, WIDTH, 8);
    fill_rect(frame, 0, 0, 8, HEIGHT);
    fill_rect(frame, WIDTH - 8, 0, 8, HEIGHT);

    // TOP LEFT: one large block.
    fill_rect(frame, 32, 32, 80, 80);

    // TOP RIGHT: two vertical bars.
    fill_rect(frame, WIDTH - 128, 32, 24, 80);
    fill_rect(frame, WIDTH - 80, 32, 24, 80);

    // BOTTOM LEFT: three horizontal bars.
    for index in 0..3 {
        fill_rect(frame, 32, HEIGHT - 128 + index * 32, 80, 16);
    }

    // BOTTOM RIGHT: four squares.
    for row in 0..2 {
        for column in 0..2 {
            fill_rect(
                frame,
                WIDTH - 128 + column * 48,
                HEIGHT - 128 + row * 48,
                32,
                32,
            );
        }
    }

    // Center cross.
    fill_rect(frame, WIDTH / 2 - 60, HEIGHT / 2 - 4, 120, 8);
    fill_rect(frame, WIDTH / 2 - 4, HEIGHT / 2 - 60, 8, 120);
}

fn fill_rect(frame: &mut [u8; FRAMEBUFFER_LEN], x: usize, y: usize, width: usize, height: usize) {
    let right = (x + width).min(WIDTH);
    let bottom = (y + height).min(HEIGHT);

    for py in y..bottom {
        for px in x..right {
            set_black(frame, px, py);
        }
    }
}

fn set_black(frame: &mut [u8; FRAMEBUFFER_LEN], x: usize, y: usize) {
    let index = y * STRIDE + x / 8;
    let mask = 0x80 >> (x % 8);

    frame[index] &= !mask;
}
