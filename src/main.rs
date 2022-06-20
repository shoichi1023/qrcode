use std::{
    collections::VecDeque,
    fs::File,
    io::{BufRead, BufReader},
};

use anyhow::{Ok, Result};
use clap::{Parser, Subcommand};
use indicatif::ProgressBar;
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
    /// generate QR Code
    Generate {
        /// url list csv path like name,url
        #[clap(short, long, value_parser)]
        url: String,
        /// The size of one side of the image
        #[clap(short, long, value_parser, default_value_t = 300)]
        size: i32,
        /// # is replaced by QR name
        #[clap(short, long, value_parser, default_value_t = String::from("./#_qrcode.png"))]
        output: String,
    },
    /// replace QR Code on video
    Replace {
        /// input file path
        #[clap(short, long, value_parser)]
        input: String,
        /// url list csv path like name,url
        #[clap(short, long, value_parser)]
        url: String,
        /// QR Code padding.  please increase, If the QR Code is resized in the middle of the video.
        #[clap(short, long, value_parser, default_value_t = 15)]
        padding: i32,
        /// output file path, # is replaced by QR name
        #[clap(short, long, value_parser, default_value_t = String::from("./#_replaced.mp4"))]
        output: String,
    },
    /// replace QR Code on image
    ImgReplace {
        /// input file path
        #[clap(short, long, value_parser)]
        input: String,
        /// url list csv path like name,url
        #[clap(short, long, value_parser)]
        url: String,
        /// QR Code padding.  please increase, If the old QR Code remains.
        #[clap(short, long, value_parser, default_value_t = 15)]
        padding: i32,
        /// output file path, # is replaced by QR name
        #[clap(short, long, value_parser, default_value_t = String::from("./#_replaced.png"))]
        output: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Generate { url, size, output } => {
            let qr_size = opencv::core::Size::new(*size, *size);
            let qr_code_list = qr_code_generate(url, qr_size)?;
            for x in qr_code_list {
                imgcodecs::imwrite(
                    &output.replace("#", &x.0),
                    &x.1,
                    &opencv::core::Vector::default(),
                )?;
            }
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
            println!("{}", replaced.len());
            for x in replaced {
                imgcodecs::imwrite(
                    &output.replace("#", &x.0),
                    &x.1,
                    &opencv::core::Vector::default(),
                )?;
            }
        }
    }

    Ok(())
}

// 動画のQRコードを置換するためのwrapper
fn video_qr_code_replace(
    input: &String,
    url: &String,
    padding: i32,
    output: &String,
) -> Result<()> {
    let start = std::time::Instant::now();

    //初期設定
    let mut video = videoio::VideoCapture::from_file(&input, videoio::CAP_ANY)?;
    let fourcc = video.get(videoio::CAP_PROP_FOURCC)? as i32;
    let frame_count = video.get(videoio::CAP_PROP_FRAME_COUNT)? as i32;
    let fps = video.get(videoio::CAP_PROP_FPS)?;
    let frame_size = opencv::core::Size::new(
        video.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32,
        video.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32,
    );
    let max_miss_count = fps as i32 / 2;
    let pb = ProgressBar::new(frame_count as u64);

    // ファイル取得
    let mut replaced_video_list: Vec<videoio::VideoWriter> = Vec::new();
    let mut name_list: Vec<String> = Vec::new();
    let f = File::open(url).unwrap();
    let reader = BufReader::new(f);
    for line in reader.lines() {
        let line = line.unwrap().split(",").fold(Vec::new(), |mut s, i| {
            s.push(i.to_string());
            s
        });
        name_list.push(line[0].clone());
        replaced_video_list.push(videoio::VideoWriter::new(
            &output.replace("#", &line[0]),
            fourcc,
            fps,
            frame_size,
            true,
        )?)
    }
    let mut detected_future_frame_rect: VecDeque<opencv::core::Rect> = VecDeque::new();
    let mut detected_rect: Option<opencv::core::Rect> = None;
    for i in 0..frame_count {
        let mut target_img = opencv::core::Mat::default();
        video.set(videoio::CAP_PROP_POS_FRAMES, i as f64)?;
        video.read(&mut target_img)?;
        let replace_rect = if detected_future_frame_rect.is_empty() {
            img_qr_code_detect(&target_img, padding)?
        } else {
            detected_future_frame_rect.pop_front()
        };
        let replaced_img_list = match replace_rect {
            Some(rect) => {
                if detected_rect.is_none() {
                    detected_rect = Some(rect)
                };
                img_qr_code_replace(target_img, url, detected_rect.unwrap())?
            }
            None => {
                let mut found_rect: Option<opencv::core::Rect> = None;
                for j in 0..max_miss_count {
                    if i + j >= frame_count {
                        break;
                    }
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
                    if detected_rect.is_none() {
                        detected_rect = found_rect;
                    };
                    img_qr_code_replace(target_img, url, detected_rect.unwrap())?
                } else {
                    name_list
                        .clone()
                        .into_iter()
                        .map(|n| (n, target_img.clone()))
                        .collect()
                }
            }
        };

        let _: Result<()> = replaced_video_list
            .iter_mut()
            .zip(replaced_img_list.iter())
            .map(|x| -> Result<()> {
                x.0.write(&x.1 .1)?;
                Ok(())
            })
            .collect();
        pb.inc(1);
    }

    let end = start.elapsed();
    pb.finish_and_clear();
    println!(
        "Done ( {}.{:03} sec )",
        end.as_secs(),
        end.subsec_nanos() / 1_000_000
    );

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

    if rect.width < 0 || rect.height < 0 {
        return Ok(None);
    }

    Ok(Some(rect))
}

// QRコード置換
fn img_qr_code_replace(
    input: opencv::core::Mat,
    url: &String,
    rect: opencv::core::Rect,
) -> Result<Vec<(String, opencv::core::Mat)>> {
    // QRコードの置換

    let qr_size = opencv::core::Size::new(rect.width, rect.height);
    let new_qr_list = qr_code_generate(url, qr_size)?;
    let mat_list: Vec<opencv::core::Mat> = new_qr_list
        .iter()
        .map(|_| opencv::core::Mat::from(input.clone()))
        .collect();
    Ok(mat_list
        .into_iter()
        .zip(new_qr_list.into_iter())
        .map(|x| -> Result<(String, opencv::core::Mat)> {
            let mut cover = opencv::core::Mat::roi(&x.0, rect)?;
            x.1 .1.copy_to(&mut cover)?;
            Ok((x.1 .0, x.0))
        })
        .flatten()
        .collect())
}

// QRコード生成
fn qr_code_generate(
    url: &String,
    size: opencv::core::Size,
) -> Result<Vec<(String, opencv::core::Mat)>> {
    let mut new_qr_list: Vec<(String, opencv::core::Mat)> = Vec::new();
    let f = File::open(url).unwrap();
    let reader = BufReader::new(f);
    for line in reader.lines() {
        let line = line.unwrap().split(",").fold(Vec::new(), |mut s, i| {
            s.push(i.to_string());
            s
        });
        let name = line[0].clone();
        let qr_url = line[1].clone();
        let mut new_qr = opencv::core::Mat::default();
        let mut qr_code_params = objdetect::QRCodeEncoder_Params::default()?;
        qr_code_params.version = 4;
        let mut encoder: PtrOfQRCodeEncoder =
            <dyn objdetect::QRCodeEncoder>::create(qr_code_params)?;
        encoder.encode(&qr_url, &mut new_qr)?;
        imgproc::resize(
            &new_qr.clone(),
            &mut new_qr,
            size,
            0.0,
            0.0,
            imgproc::INTER_AREA,
        )?;
        imgproc::cvt_color(&new_qr.clone(), &mut new_qr, imgproc::COLOR_GRAY2BGR, 0)?;
        new_qr_list.push((name, new_qr));
    }

    Ok(new_qr_list)
}
