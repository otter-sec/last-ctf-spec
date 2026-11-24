use bevy::color::LinearRgba;

pub const TEXT: LinearRgba = LinearRgba::rgb(1.4, 1.4, 1.4);
pub const DIM_TEXT: LinearRgba = LinearRgba::rgb(0.45, 0.45, 0.5);
pub const BORDER: LinearRgba = LinearRgba::rgb(0.12, 0.12, 0.18);
pub const STAR: LinearRgba = LinearRgba::rgb(0.18, 0.18, 0.22);
pub const ASTEROID: LinearRgba = LinearRgba::rgb(1.1, 1.1, 1.2);
pub const FLAME: LinearRgba = LinearRgba::rgb(2.6, 1.3, 0.3);

const SLOTS: [LinearRgba; 4] = [
    LinearRgba::rgb(0.25, 2.0, 2.6),
    LinearRgba::rgb(2.6, 0.35, 1.7),
    LinearRgba::rgb(2.4, 2.1, 0.25),
    LinearRgba::rgb(0.45, 2.6, 0.55),
];

pub fn slot(slot: u8) -> LinearRgba {
    SLOTS[slot as usize % SLOTS.len()]
}
