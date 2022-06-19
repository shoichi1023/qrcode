use std::collections::VecDeque;

use anyhow::{Ok, Result};
use clap::{Parser, Subcommand};
use opencv::{
    imgcodecs, imgproc, objdetect,
    prelude::{MatTraitConst, QRCodeDetectorTraitConst, QRCodeEncoder},
    types::{PtrOfQRCodeEncoder, VectorOfPoint},
    videoio::{self, VideoCaptureTrait, VideoCaptureTraitConst, VideoWriterTrait},
};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Generate {
        #[clap(short, long, value_parser)]
        url: String,
        #[clap(short, long, value_parser, default_value_t = 300)]
        size: i32,
        #[clap(short, long, value_parser, default_value_t = String::from("./qrcode.png"))]
        output: String,
    },
    Replace {
        #[clap(short, long, value_parser)]
        input: String,
        #[clap(short, long, value_parser)]
        url: String,
        #[clap(short, long, value_parser, default_value_t = 15)]
        padding: i32,
        #[clap(short, long, value_parser, default_value_t = String::from("./replaced.mp4"))]
        output: String,
    },
    ImgReplace {
        #[clap(short, long, value_parser)]
        input: String,
        #[clap(short, long, value_parser)]
        url: String,
        #[clap(short, long, value_parser, default_value_t = 15)]
        padding: i32,
        #[clap(short, long, value_parser, default_value_t = String::from("./replaced.png"))]
        output: String,
    },
}

fn main() -> Result<()> {
    let start = std::time::Instant::now();
    let cli = Cli::parse();

    match &cli.command {
        Commands::Generate { url, size, output } => {
            let qr_size = opencv::core::Size::new(*size, *size);
            let qr_code = qr_code_generate(url, qr_size)?;
            imgcodecs::imwrite(&output, &qr_code, &opencv::core::Vector::default())?;
        }
        Commands::Replace {
            input,
            url,
            padding,
            output,
        } => {
            video_qr_code_replace(input, url, *padding, output)?;
        }
        Commands::ImgReplace {
            input,
            url,
            padding,
            output,
        } => {
            let target_img = imgcodecs::imread(input, imgcodecs::IMREAD_COLOR)?;
            let qr_rect = img_qr_code_detect(&target_img, *padding)?;
            if qr_rect.is_none() {
                eprintln!("QRコードが検出できませんでした。");
                std::process::exit(1)
            }
            let replaced = img_qr_code_replace(target_img, url, qr_rect.unwrap())?;
            imgcodecs::imwrite(&output, &replaced, &opencv::core::Vector::default())?;
        }
    }

    let end = start.elapsed();
    println!(
        "実行時間 : {}.{:03} sec",
        end.as_secs(),
        end.subsec_nanos() / 1_000_000
    );

    Ok(())
}

// 動画のQRコードを置換するためのwrapper
fn video_qr_code_replace(
    input: &String,
    url: &String,
    padding: i32,
    output: &String,
) -> Result<()> {
    let mut video = videoio::VideoCapture::from_file(&input, videoio::CAP_ANY)?;
    let fourcc = video.get(videoio::CAP_PROP_FOURCC)? as i32;
    let frame_count = video.get(videoio::CAP_PROP_FRAME_COUNT)? as i32;
    let fps = video.get(videoio::CAP_PROP_FPS)?;
    let frame_size = opencv::core::Size::new(
        video.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32,
        video.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32,
    );
    let max_miss_count = fps as i32 / 2;
    let mut replaced_video = videoio::VideoWriter::new(&output, fourcc, fps, frame_size, true)?;

    let mut detected_future_frame_rect: VecDeque<opencv::core::Rect> = VecDeque::new();
    for i in 0..frame_count {
        let mut target_img = opencv::core::Mat::default();
        video.set(videoio::CAP_PROP_POS_FRAMES, i as f64)?;
        video.read(&mut target_img)?;
        let replace_rect = if detected_future_frame_rect.is_empty() {
            img_qr_code_detect(&target_img, padding)?
        } else {
            detected_future_frame_rect.pop_front()
        };
        let replaced_img = match replace_rect {
            Some(rect) => img_qr_code_replace(target_img, url, rect)?,
            None => {
                let mut found_rect: Option<opencv::core::Rect> = None;
                for j in 0..max_miss_count {
                    video.set(videoio::CAP_PROP_POS_FRAMES, (i + j) as f64)?;
                    video.read(&mut target_img)?;
                    found_rect = img_qr_code_detect(&target_img, padding)?;
                    if found_rect.is_some() {
                        for _l in 0..j + 1 {
                            detected_future_frame_rect.push_back(found_rect.unwrap());
                        }
                        break;
                    }
                }
                if found_rect.is_some() {
                    img_qr_code_replace(target_img, url, found_rect.unwrap())?
                } else {
                    target_img
                }
            }
        };

        replaced_video.write(&replaced_img)?;
    }

    Ok(())
}

// QRコードを検出
fn img_qr_code_detect(
    input: &opencv::core::Mat,
    padding: i32,
) -> Result<Option<opencv::core::Rect>> {
    // QRコードを検出
    let mut result = VectorOfPoint::new();
    let qr_detecter = objdetect::QRCodeDetector::default()?;
    let is_detected = qr_detecter.detect(&input, &mut result)?;

    if !is_detected {
        return Ok(None);
    }

    // 検出位置の取得
    let top_left_point = result.get(0)?;
    let bottom_right_point = result.get(2)?;
    let rect = opencv::core::Rect::new(
        top_left_point.x - padding,
        top_left_point.y - padding,
        bottom_right_point.x - top_left_point.x + padding * 2,
        bottom_right_point.y - top_left_point.y + padding * 2,
    );

    Ok(Some(rect))
}

// QRコード置換
fn img_qr_code_replace(
    input: opencv::core::Mat,
    url: &String,
    rect: opencv::core::Rect,
) -> Result<opencv::core::Mat> {
    // QRコードの置換
    let mut cover = opencv::core::Mat::roi(&input, rect)?;
    let qr_size = opencv::core::Size::new(rect.width, rect.height);
    let new_qr = qr_code_generate(url, qr_size)?;
    new_qr.copy_to(&mut cover)?;

    Ok(input)
}

// QRコード生成
fn qr_code_generate(url: &String, size: opencv::core::Size) -> Result<opencv::core::Mat> {
    let mut new_qr = opencv::core::Mat::default();
    let mut qr_code_params = objdetect::QRCodeEncoder_Params::default()?;
    qr_code_params.version = 4;
    let mut encoder: PtrOfQRCodeEncoder = <dyn objdetect::QRCodeEncoder>::create(qr_code_params)?;
    encoder.encode(&url, &mut new_qr)?;
    imgproc::resize(
        &new_qr.clone(),
        &mut new_qr,
        size,
        0.0,
        0.0,
        imgproc::INTER_AREA,
    )?;
    imgproc::cvt_color(&new_qr.clone(), &mut new_qr, imgproc::COLOR_GRAY2BGR, 0)?;
    Ok(new_qr)
}
