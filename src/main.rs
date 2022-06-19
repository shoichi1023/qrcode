use anyhow::{Ok, Result};
use opencv::{
    imgcodecs, imgproc, objdetect,
    prelude::{QRCodeDetectorTraitConst, QRCodeEncoder},
    types::{PtrOfQRCodeEncoder, VectorOfPoint},
};
fn main() -> Result<()> {
    // 初期設定
    let target_img = imgcodecs::imread("./assets/target.png", imgcodecs::IMREAD_COLOR)?;
    let replace_qr_data = "https://github.com/shoichi1023";
    let padding = 15;
    let qr_code_version = 4;

    // QRコードを検出
    let mut result = VectorOfPoint::new();
    let qr_detecter = objdetect::QRCodeDetector::default()?;
    let is_detected = qr_detecter.detect(&target_img, &mut result)?;
    if !is_detected {
        eprintln!("Error : QRコードが検出できませんでした。");
        std::process::exit(1);
    }

    // 検出位置の取得
    let top_left_point = result.get(0)?;
    let bottom_right_point = result.get(2)?;
    let qr_size = opencv::core::Size::new(
        bottom_right_point.x - top_left_point.x + padding * 2,
        bottom_right_point.y - top_left_point.y + padding * 2,
    );
    let rect = opencv::core::Rect::new(
        top_left_point.x - padding,
        top_left_point.y - padding,
        qr_size.width,
        qr_size.height,
    );

    // QRコード生成
    let mut new_qr = opencv::core::Mat::default();
    let mut qr_code_params = objdetect::QRCodeEncoder_Params::default()?;
    qr_code_params.version = qr_code_version;
    let mut encoder: PtrOfQRCodeEncoder = <dyn objdetect::QRCodeEncoder>::create(qr_code_params)?;
    encoder.encode(replace_qr_data, &mut new_qr)?;
    imgproc::resize(
        &new_qr.clone(),
        &mut new_qr,
        qr_size,
        0.0,
        0.0,
        imgproc::INTER_AREA,
    )?;
    imgproc::cvt_color(&new_qr.clone(), &mut new_qr, imgproc::COLOR_GRAY2BGR, 0)?;

    // QRコードの置換
    let mut cover = opencv::core::Mat::roi(&target_img, rect)?;
    opencv::core::add(
        &new_qr.clone(),
        &new_qr,
        &mut cover,
        &opencv::core::no_array(),
        -1,
    )?;

    // 成果物の書き出し
    imgcodecs::imwrite(
        "./assets/result.png",
        &target_img,
        &opencv::core::Vector::default(),
    )?;

    Ok(())
}
