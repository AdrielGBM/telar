use super::*;

const WIDTH: u32 = 512;
const HEIGHT: u32 = 4;
// Far enough from either end that the widest blur below never runs out of pixels and has its window truncated.
const EDGE: usize = 256;

// An opaque half-plane, blurred. Every row carries the same profile, so one row is the whole answer.
fn blurred_edge(sigma: f32) -> Vec<u8> {
    let mut data = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    for y in 0..HEIGHT as usize {
        let row = y * WIDTH as usize;
        for x in 0..EDGE {
            data[(row + x) * 4..(row + x) * 4 + 4].fill(255);
        }
    }
    gaussian_blur(&mut data, WIDTH, HEIGHT, sigma, &mut Vec::new());
    let row = (HEIGHT as usize / 2) * WIDTH as usize;
    (0..WIDTH as usize)
        .map(|x| data[(row + x) * 4 + 3])
        .collect()
}

// Where the falling profile passes `level`, interpolated between the two samples either side of it: taking the nearer sample instead biases every crossing half a pixel outwards, which is half a deviation at the radii that matter.
fn crossing(profile: &[u8], level: f32) -> f32 {
    let at = profile
        .iter()
        .position(|&alpha| f32::from(alpha) <= level)
        .expect("the profile falls to the level");
    let (above, below) = (f32::from(profile[at - 1]), f32::from(profile[at]));
    at as f32 - (level - below) / (above - below)
}

// A Gaussian across a step edge passes 84% of full one deviation inside it and 16% one deviation outside, so its two crossings sit two deviations apart.
fn measured_sigma(profile: &[u8]) -> f32 {
    (crossing(profile, 0.16 * 255.0) - crossing(profile, 0.84 * 255.0)) / 2.0
}

fn painted_extent(profile: &[u8]) -> usize {
    let last = profile
        .iter()
        .rposition(|&alpha| alpha > 0)
        .expect("the blur reaches past the edge");
    last + 1 - EDGE
}

/// Three box passes have to stand in for the deviation they were handed, not half again as much.
///
/// They were given `1.5 * sigma` as their radius, which is the radius of *one* pass approximating that deviation; three of them compound to about `1.5 * sigma` of deviation instead. Below a radius of 4 the assertion is dropped rather than loosened: the passes are integers and the narrowest, `r = 1`, already stands for `sqrt(2)`, so nothing under that deviation can be expressed at all.
#[test]
fn three_box_passes_stand_in_for_the_deviation_they_were_asked_for() {
    for radius in [4.0f32, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0, 64.0] {
        let sigma = renderer_core::blur_sigma(radius);
        let measured = measured_sigma(&blurred_edge(sigma));
        // Half a pixel for a radius that has to be a whole one, and 2.5% for three boxes and a Gaussian of the same variance not passing these two quantiles at quite the same place. Half again as much deviation, which is what this is here to catch, is out by 50%.
        let tolerance = 0.6 + 0.025 * sigma;
        assert!(
            (measured - sigma).abs() <= tolerance,
            "radius {radius} asked for a deviation of {sigma} and got {measured}, past the {tolerance} this may be out by"
        );
    }
}

/// And they have to stay inside the room reserved for them.
///
/// [`blur_padding`](renderer_core::blur_padding) is what every shadow's pixmap is cut to and what the dirty tracker pads a blurred region by, both measured from the same deviation. A blur that reaches further is clipped by the edge of its own padding — the tail of every shadow cut off square — and paints where nothing damaged for it.
#[test]
fn a_blur_paints_no_further_than_the_padding_reserved_for_it() {
    for radius in [
        1.0f32, 2.0, 3.0, 4.0, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0, 64.0,
    ] {
        let sigma = renderer_core::blur_sigma(radius);
        let extent = painted_extent(&blurred_edge(sigma));
        let padding = renderer_core::blur_padding(sigma) as usize;
        assert!(
            extent <= padding,
            "radius {radius} painted {extent} px past the edge with {padding} px reserved"
        );
        // Only a blur that stopped working could pass the line above on its own; what the deviation should be is the case above's to say.
        assert!(
            extent * 3 >= padding,
            "radius {radius} painted {extent} px into {padding} px of padding, which is no blur at all"
        );
    }
}
