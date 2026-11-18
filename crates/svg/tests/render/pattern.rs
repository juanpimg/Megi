use super::*;

#[test]
fn test_huge_pattern_phase_does_not_panic() {
    let source = b"<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'>        <defs><pattern id='p' width='1' height='1' patternUnits='userSpaceOnUse'         patternTransform='matrix(0.01 0 0 0.01 1e19 0)'><rect width='1' height='1' fill='red'/>        </pattern></defs><rect width='8' height='8' fill='url(#p)'/></svg>";
    let image = render_svg_with_limits(source, &SvgOptions::default(), &Limits::default())
        .expect("a huge but valid pattern phase must not fail");
    assert_eq!((image.width, image.height), (8, 8));
}

#[test]
fn test_pattern_phase_beyond_the_tile_budget_still_renders() {
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'>        <defs><pattern id='p' width='1' height='1' patternUnits='userSpaceOnUse'         patternTransform='matrix(1 0 0 1 20000 0)'><rect width='1' height='1' fill='red'/>        </pattern></defs><rect width='8' height='8' fill='url(#p)'/></svg>";
    let image = render_at(source, 8, 8);
    assert_eq!(pixel(&image, 0, 0), [255, 0, 0, 255]);
}
