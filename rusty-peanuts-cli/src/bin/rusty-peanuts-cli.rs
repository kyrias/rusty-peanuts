use std::io::Seek;

use async_std::task::JoinHandle;
use futures_lite::stream::StreamExt;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use structopt::StructOpt;
use surf::StatusCode;

use rusty_peanuts_api_structs::{PhotoPayload, Source};
use rusty_peanuts_cli::xmp::get_metadata;

#[derive(StructOpt)]
struct SharedApiArgs {
    /// Rusty Peanuts API host
    #[structopt(long, env = "RUSTY_PEANUTS_API_ENDPOINT")]
    endpoint: String,
    /// Rusty Peanuts API secret key
    #[structopt(long, env = "RUSTY_PEANUTS_API_SECRET_KEY", hide_env_values = true)]
    secret_key: String,
}

#[derive(StructOpt)]
pub struct UploadArgs {
    #[structopt(flatten)]
    api_arguments: SharedApiArgs,

    /// Full S3-compatible region endpoint.
    #[structopt(long, env = "RUSTY_PEANUTS_S3_REGION_ENDPOINT")]
    s3_region_endpoint: String,

    /// S3-compatible bucket name.
    #[structopt(long, env = "RUSTY_PEANUTS_S3_BUCKET")]
    s3_bucket: String,

    /// S3 access key ID.
    #[structopt(long, env = "RUSTY_PEANUTS_S3_ACCESS_KEY_ID", hide_env_values = true)]
    s3_access_key_id: String,
    /// S3 secret access key
    #[structopt(
        long,
        env = "RUSTY_PEANUTS_S3_SECRET_ACCESS_KEY",
        hide_env_values = true
    )]
    s3_secret_access_key: String,

    /// Hostname to use when displaying files uploaded to S3-compatible storage.
    #[structopt(long, env = "RUSTY_PEANUTS_STATIC_HOST")]
    static_host: String,

    /// Only update metadata.
    #[structopt(long)]
    only_update_metadata: bool,

    /// Path to photo file to upload.
    #[structopt(name = "PATH", parse(from_os_str))]
    file_path: std::path::PathBuf,
}

#[derive(StructOpt)]
pub struct SetPublishedArgs {
    #[structopt(flatten)]
    api_arguments: SharedApiArgs,

    /// Photo ID to change published state on.
    #[structopt(name = "PHOTO_ID")]
    photo_id: u32,

    /// Whether to publish or unpublish the photo.
    #[structopt(name = "PUBLISHED", parse(try_from_str))]
    published: bool,
}

#[derive(StructOpt)]
pub struct SetHeightOffsetArgs {
    #[structopt(flatten)]
    api_arguments: SharedApiArgs,

    /// Photo ID to change published state on.
    #[structopt(name = "PHOTO_ID")]
    photo_id: u32,

    /// Whether to publish or unpublish the photo.
    #[structopt(name = "HEIGHT_OFFSET")]
    height_offset: u8,
}

#[derive(StructOpt)]
pub enum Command {
    Upload(UploadArgs),
    Update(UploadArgs),
    SetPublished(SetPublishedArgs),
    SetHeightOffset(SetHeightOffsetArgs),
}

fn decode_image(file: &std::fs::File) -> (image::DynamicImage, image::ImageFormat) {
    let bufreader = std::io::BufReader::new(file);
    let reader = image::io::Reader::new(bufreader);
    let reader = reader
        .with_guessed_format()
        .expect("couldn't guess file format");

    let format = reader.format().expect("couldn't get guessed file format");
    let image = reader.decode().expect("couldn't decode file");

    (image, format)
}

fn encode_jpeg(image: &image::DynamicImage) -> (Vec<u8>, u32, u32) {
    let rgb_image = image.to_rgb8();
    let (width, height) = (rgb_image.width(), rgb_image.height());
    let rgb_data = rgb_image.into_vec();
    log::debug!("Turned image into raw RGB data");

    let mut compress = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_EXT_RGB);
    compress.set_size(width as usize, height as usize);
    compress.set_quality(80.0);
    compress.set_progressive_mode();
    compress.set_scan_optimization_mode(mozjpeg::ScanMode::AllComponentsTogether);
    compress.set_optimize_scans(true);
    compress.set_mem_dest();

    compress.start_compress();
    log::debug!("Started compressing image");

    compress.write_scanlines(&rgb_data);
    log::debug!("Wrote scanlines");

    compress.finish_compress();
    log::debug!("Finished compressing image");

    let data = compress
        .data_to_vec()
        .expect("couldn't convert compressed image data to vector");
    (data, width, height)
}

fn transcode_photo(image: image::DynamicImage) -> Vec<JoinHandle<(Vec<u8>, u32, u32)>> {
    // Filter out the target sizes to only contain those less than or equal to the largest of the
    // photo's dimensions.
    let (width, height) = (image.width(), image.height());
    let sizes = [
        1800, 1700, 1600, 1500, 1400, 1300, 1200, 1100, 1000, 900, 800, 700, 600, 500, 400, 300,
    ]
    .iter()
    .filter(move |&&s| s <= std::cmp::max(width, height));

    let mut handles = Vec::new();
    for size in sizes {
        let image = image.clone();

        let handle = async_std::task::spawn_blocking(move || {
            let start = std::time::Instant::now();
            log::info!("Started resizing image to {}px", size);
            let resized = image.resize(*size, *size, image::imageops::FilterType::Lanczos3);
            log::info!(
                "Finished resizing image to {}px in {}s",
                size,
                start.elapsed().as_secs_f32()
            );

            let (jpeg_data, width, height) = encode_jpeg(&resized);
            log::info!(
                "Finished image of size {}px in {}s",
                size,
                start.elapsed().as_secs_f32()
            );

            (jpeg_data, width, height)
        });
        handles.push(handle);
    }

    handles
}

async fn upload_transcoded_photo(
    args: &UploadArgs,
    bucket: &Bucket,
    file_stem: &str,
    data: Vec<u8>,
    width: u32,
    height: u32,
) -> Source {
    log::info!("Uploading resized image of size {}x{}", width, height);

    let target_path = format!("{}/{}.{}x{}.jpeg", file_stem, file_stem, width, height);
    let response = bucket
        .put_object_with_content_type(&target_path, &data, "image/jpeg")
        .await
        .expect("could not upload file");
    let code = response.status_code();
    assert!(code >= 200 && code < 300);
    log::info!(
        "Uploading resized image of size {}x{} finished",
        width,
        height
    );

    Source {
        width: width,
        height: height,
        url: format!("{}/{}", args.static_host, target_path),
    }
}

async fn upload_photo(args: UploadArgs, update: bool) -> std::io::Result<()> {
    let auth_header = format!("Bearer {}", args.api_arguments.secret_key);
    let file_stem = args
        .file_path
        .file_stem()
        .expect("couldn't get file stem from path")
        .to_string_lossy();

    if !update {
        log::info!("Checking whether photo has already been uploaded");
        let url = format!(
            "{}/api/v1/photo/by-filestem/{}",
            args.api_arguments.endpoint, file_stem
        );
        let res = surf::get(url)
            .header("Authorization", &auth_header)
            .await
            .expect("couldn't send GET request to rusty-peanuts API");
        if res.status() == StatusCode::Ok {
            log::error!("Photo with filestem {} already exists", file_stem);
            std::process::exit(1);
        }
    }

    let credentials = Credentials::new(
        Some(&args.s3_access_key_id),
        Some(&args.s3_secret_access_key),
        None,
        None,
        None,
    )
    .expect("couldn't create S3 credentials instance");

    let s3_region_name = args
        .s3_region_endpoint
        .splitn(2, ".")
        .next()
        .expect("couldn't get region name from region endpoint")
        .to_string();

    let mut bucket = Bucket::new(
        &args.s3_bucket,
        s3::Region::Custom {
            region: s3_region_name,
            endpoint: args.s3_region_endpoint.clone(),
        },
        credentials,
    )
    .expect("couldn't create S3 bucket instance")
    .with_path_style();
    bucket.add_header("x-amz-acl", "public-read");
    bucket.add_header("Cache-Control", "max-age=31536000");

    let mut file = std::fs::File::open(&args.file_path).expect("couldn't open photo file");

    let (image, format) = decode_image(&file);
    file.seek(std::io::SeekFrom::Start(0))
        .expect("couldn't seek file to begining");

    let image_create_datetime: String;
    let image_title: Option<String>;
    let image_tags: Vec<String>;

    match format {
        image::ImageFormat::Tiff => {
            let (_xmp_xml, create_date, title, tags) = get_metadata(&file);
            image_create_datetime = create_date;
            image_title = title;
            image_tags = tags;
        },
        _ => {
            log::error!("Unupported format: {:?}", format);
            std::process::exit(1);
        },
    }

    let sources = if args.only_update_metadata {
        log::info!("Not uploading photos");
        None
    } else {
        let sources = async_std::stream::from_iter(transcode_photo(image).into_iter())
            .then(|handle: JoinHandle<_>| handle)
            .then(|(data, width, height)| {
                upload_transcoded_photo(&args, &bucket, &file_stem, data, width, height)
            })
            .collect()
            .await;
        log::info!("All images uploaded");
        Some(sources)
    };

    let payload = PhotoPayload {
        file_stem: file_stem.to_string(),
        taken_timestamp: Some(image_create_datetime),
        title: image_title,
        tags: image_tags,
        sources: sources,
    };

    log::info!("Sending photo payload to rusty-peanuts API");
    let url = if update {
        format!(
            "{}/api/v1/photo/by-filestem/{}",
            args.api_arguments.endpoint, file_stem
        )
    } else {
        format!("{}/api/v1/photos", args.api_arguments.endpoint)
    };
    let mut res = surf::post(url)
        .header("Authorization", &auth_header)
        .body(surf::Body::from_json(&payload).expect("couldn't serialize body"))
        .await
        .expect("couldn't send POST request to rusty-peanuts API");
    let body: serde_json::Value = res.body_json().await.unwrap();
    log::info!("Rusty-peanuts API response: {:#?}", res);
    log::info!("Rusty-peanuts API body: {:#?}", body);

    let status = res.status();
    assert!(!status.is_client_error() && !status.is_server_error());

    Ok(())
}

async fn set_published(args: SetPublishedArgs) -> std::io::Result<()> {
    let url = format!(
        "{}/api/v1/photo/by-id/{}/published",
        args.api_arguments.endpoint, args.photo_id,
    );
    let res = surf::post(url)
        .header(
            "Authorization",
            format!("Bearer {}", args.api_arguments.secret_key),
        )
        .body(surf::Body::from_json(&args.published).expect("couldn't serialize body"))
        .await
        .expect("couldn't send POST request to rusty-peanuts API");
    log::info!("Rusty-peanuts API response: {:#?}", res);

    Ok(())
}

async fn set_height_offset(args: SetHeightOffsetArgs) -> std::io::Result<()> {
    let url = format!(
        "{}/api/v1/photo/by-id/{}/height-offset",
        args.api_arguments.endpoint, args.photo_id,
    );
    let res = surf::post(url)
        .header(
            "Authorization",
            format!("Bearer {}", args.api_arguments.secret_key),
        )
        .body(surf::Body::from_json(&args.height_offset).expect("couldn't serialize body"))
        .await
        .expect("couldn't send POST request to rusty-peanuts API");
    log::info!("Rusty-peanuts API response: {:#?}", res);

    Ok(())
}

#[async_std::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    match Command::from_args() {
        Command::Upload(args) => upload_photo(args, false).await,
        Command::Update(args) => upload_photo(args, true).await,
        Command::SetPublished(args) => set_published(args).await,
        Command::SetHeightOffset(args) => set_height_offset(args).await,
    }
}
