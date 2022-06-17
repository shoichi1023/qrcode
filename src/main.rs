use anyhow::{Ok, Result};
use opencv::{
    imgcodecs,
    imgproc::{self, TemplateMatchModes},
    prelude::MatTraitConstManual,
};
fn main() -> Result<()> {
    let mut target_img = imgcodecs::imread("./assets/target.png", imgcodecs::IMREAD_COLOR)?;
    let search_img = imgcodecs::imread("./assets/replace.png", imgcodecs::IMREAD_COLOR)?;
    let mut result = opencv::core::Mat::default();
    imgproc::match_template(
        &target_img,
        &search_img,
        &mut result,
        TemplateMatchModes::TM_SQDIFF_NORMED as i32,
        &opencv::core::no_array(),
    )?;
    let search_size = search_img.size()?;
    let mut max_val = 0.0;
    let mut max_loc = opencv::core::Point::default();
    opencv::core::min_max_loc(
        &result,
        None,
        Some(&mut max_val),
        None,
        Some(&mut max_loc),
        &opencv::core::no_array(),
    )?;
    println!("一致率 : {}", max_val);
    println!("x : {}", max_loc.x);
    println!("y : {}", max_loc.y);
    let rect = opencv::core::Rect::new(max_loc.x, max_loc.y, search_size.width, search_size.height);
    imgproc::rectangle(
        &mut target_img,
        rect,
        opencv::core::Scalar::new(0.0, 0.0, 255.0, 1.0),
        10,
        imgproc::LINE_8,
        0,
    )?;
    imgcodecs::imwrite(
        "./assets/result.png",
        &target_img,
        &opencv::core::Vector::default(),
    )?;
    Ok(())
}
