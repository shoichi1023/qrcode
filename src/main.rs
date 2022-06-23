use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use anyhow::{Ok, Result};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
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
            let start_time = std::time::Instant::now();
            let mut video = videoio::VideoCapture::from_file(&input, videoio::CAP_ANY)?;
            let fps = video.get(videoio::CAP_PROP_FPS)?;
            let predict_miss_count = fps as i32 / 5;
            let (rect, start) = video_qr_code_detect(&mut video, *padding, false)?;
            let (_, end) = video_qr_code_detect(&mut video, *padding, true)?;
            video_qr_code_replace(
                &mut video,
                url,
                output,
                rect,
                start - predict_miss_count,
                end + predict_miss_count,
            )?;

            let end_time = start_time.elapsed();
            println!(
                "Done ( {}.{:03} sec )",
                end_time.as_secs(),
                end_time.subsec_nanos() / 1_000_000
            );
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
            let qr_size = opencv::core::Size::new(qr_rect.unwrap().width, qr_rect.unwrap().height);
            let new_qr_list = qr_code_generate(url, qr_size)?;
            let replaced = img_qr_code_replace(target_img, new_qr_list, qr_rect.unwrap())?;
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

// 初めにQRコードが見つかる場所を検知
fn video_qr_code_detect(
    video: &mut videoio::VideoCapture,
    padding: i32,
    is_reverse: bool,
) -> Result<(opencv::core::Rect, i32)> {
    let pb = ProgressBar::new_spinner()
        .with_style(
            ProgressStyle::default_spinner()
                .template("{prefix:.bold.dim} {spinner} {msg}")
                .tick_strings(&[
                    "▹▹▹▹▹",
                    "▸▹▹▹▹",
                    "▹▸▹▹▹",
                    "▹▹▸▹▹",
                    "▹▹▹▸▹",
                    "▹▹▹▹▸",
                    "▪▪▪▪▪",
                ]),
        )
        .with_prefix(format!("[{}/3]", is_reverse as i32 + 1))
        .with_message(if is_reverse {
            "QRコードの終了位置を検索しています..."
        } else {
            "QRコードの開始位置を検索しています..."
        });
    pb.enable_steady_tick(120);

    let frame_count = video.get(videoio::CAP_PROP_FRAME_COUNT)? as i32;

    let mut detect_frame = (opencv::core::Rect::default(), 0);
    for i in 0..frame_count {
        let mut frame_img = opencv::core::Mat::default();
        let frame_num = if is_reverse {
            (frame_count - 1 - i) as f64
        } else {
            i as f64
        };
        video.set(videoio::CAP_PROP_POS_FRAMES, frame_num)?;
        video.read(&mut frame_img)?;
        match img_qr_code_detect(&frame_img, padding)? {
            Some(rect) => {
                detect_frame = (rect, frame_num as i32);
                break;
            }
            None => {
                continue;
            }
        };
    }
    pb.finish_and_clear();
    Ok(detect_frame)
}

// 指定された区間の動画のQRコードの置換
fn video_qr_code_replace(
    video: &mut videoio::VideoCapture,
    url: &String,
    output: &String,
    rect: opencv::core::Rect,
    start: i32,
    end: i32,
) -> Result<()> {
    let fourcc = video.get(videoio::CAP_PROP_FOURCC)? as i32;
    let frame_count = video.get(videoio::CAP_PROP_FRAME_COUNT)? as i32;
    let fps = video.get(videoio::CAP_PROP_FPS)?;
    let frame_size = opencv::core::Size::new(
        video.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32,
        video.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32,
    );
    let pb = ProgressBar::new(frame_count as u64)
        .with_style(
            ProgressStyle::default_bar()
                .template("{prefix:.bold.dim} {msg} \n {bar} {pos:>4}/{len:4} "),
        )
        .with_prefix("[3/3]")
        .with_message("QRコードの置換をしています...");
    let qr_size = opencv::core::Size::new(rect.width, rect.height);
    let new_qr_list = qr_code_generate(url, qr_size)?;
    // ファイル取得
    let mut replaced_video_list: Vec<videoio::VideoWriter> = Vec::new();
    for qr in new_qr_list.iter() {
        replaced_video_list.push(videoio::VideoWriter::new(
            &output.replace("#", &qr.0),
            fourcc,
            fps,
            frame_size,
            true,
        )?)
    }
    for i in 0..frame_count {
        let mut target_img = opencv::core::Mat::default();
        video.set(videoio::CAP_PROP_POS_FRAMES, i as f64)?;
        video.read(&mut target_img)?;
        let replaced_img_list = if i >= start && i <= end {
            img_qr_code_replace(target_img, new_qr_list.clone(), rect)?
        } else {
            new_qr_list
                .clone()
                .into_iter()
                .map(|n| (n.0, target_img.clone()))
                .collect()
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

    pb.finish_and_clear();
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
    new_qr_list: Vec<(String, opencv::core::Mat)>,
    rect: opencv::core::Rect,
) -> Result<Vec<(String, opencv::core::Mat)>> {
    // QRコードの置換

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
        if line.is_empty() {
            continue;
        }
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
