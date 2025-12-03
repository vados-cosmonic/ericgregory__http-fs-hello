use wstd::http::body::{BodyForthcoming, IncomingBody};
use wstd::http::server::{Finished, Responder};
use wstd::http::{IntoBody, Request, Response, StatusCode};
use wstd::io::{copy, empty, AsyncWrite};
use wstd::time::{Duration, Instant};
use wasi::filesystem::preopens::get_directories;
use wasi::filesystem::types::{DescriptorFlags, OpenFlags, PathFlags};

#[wstd::http_server]
async fn main(req: Request<IncomingBody>, res: Responder) -> Finished {
    match req.uri().path_and_query().unwrap().as_str() {
        "/wait" => wait(req, res).await,
        "/echo" => echo(req, res).await,
        "/echo-headers" => echo_headers(req, res).await,
        "/echo-trailers" => echo_trailers(req, res).await,
        "/read-file" => read_file(req, res).await,
        "/" => home(req, res).await,
        _ => not_found(req, res).await,
    }
}

async fn home(_req: Request<IncomingBody>, res: Responder) -> Finished {
    res.respond(Response::new("Hello, wasi:http/proxy world!\n".into_body()))
        .await
}

async fn wait(_req: Request<IncomingBody>, res: Responder) -> Finished {
    let now = Instant::now();
    wstd::task::sleep(Duration::from_secs(1)).await;
    let elapsed = Instant::now().duration_since(now).as_millis();
    let mut body = res.start_response(Response::new(BodyForthcoming));
    let result = body
        .write_all(format!("slept for {elapsed} millis\n").as_bytes())
        .await;
    Finished::finish(body, result, None)
}

async fn echo(mut req: Request<IncomingBody>, res: Responder) -> Finished {
    let mut body = res.start_response(Response::new(BodyForthcoming));
    let result = copy(req.body_mut(), &mut body).await;
    Finished::finish(body, result, None)
}

async fn echo_headers(req: Request<IncomingBody>, responder: Responder) -> Finished {
    let mut res = Response::builder();
    *res.headers_mut().unwrap() = req.into_parts().0.headers;
    let res = res.body(empty()).unwrap();
    responder.respond(res).await
}

async fn echo_trailers(req: Request<IncomingBody>, res: Responder) -> Finished {
    let body = res.start_response(Response::new(BodyForthcoming));
    let (trailers, result) = match req.into_body().finish().await {
        Ok(trailers) => (trailers, Ok(())),
        Err(err) => (Default::default(), Err(std::io::Error::other(err))),
    };
    Finished::finish(body, result, trailers)
}

async fn not_found(_req: Request<IncomingBody>, responder: Responder) -> Finished {
    let res = Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(empty())
        .unwrap();
    responder.respond(res).await
}

async fn read_file(_req: Request<IncomingBody>, res: Responder) -> Finished {
    match read_text_file("sample.txt") {
        Ok(contents) => {
            res.respond(Response::new(contents.into_body())).await
        }
        Err(e) => {
            let error_msg = format!("Error reading file: {}\n", e);
            let response = Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(error_msg.into_body())
                .unwrap();
            res.respond(response).await
        }
    }
}

fn read_text_file(filename: &str) -> Result<String, String> {
    // Get preopened directories
    let preopens = get_directories();

    // Debug: Log the number of preopened directories
    let preopen_count = preopens.len();
    if preopen_count == 0 {
        return Err(format!("No preopened directories available. Volume may not be mounted correctly."));
    }

    // Debug: Log all preopened directories
    let preopen_paths: Vec<String> = preopens
        .iter()
        .map(|(_, path)| path.clone())
        .collect();

    // Find a preopened directory (typically the first one)
    let (dir_descriptor, dir_path) = preopens
        .first()
        .ok_or_else(|| format!("Found {} preopens: {:?}", preopen_count, preopen_paths))?;

    // Open the file for reading
    let path_flags = PathFlags::empty();
    let open_flags = OpenFlags::empty();
    let descriptor_flags = DescriptorFlags::READ;

    let file_descriptor = dir_descriptor
        .open_at(path_flags, filename, open_flags, descriptor_flags)
        .map_err(|e| format!("Failed to open file: {:?}", e))?;

    // Read file metadata to get size
    let stat = file_descriptor
        .stat()
        .map_err(|e| format!("Failed to get file stats: {:?}", e))?;

    let file_size = stat.size;

    // Read the file contents
    let (data, end_of_stream) = file_descriptor
        .read(file_size, 0)
        .map_err(|e| format!("Failed to read file: {:?}", e))?;

    // Convert bytes to string
    String::from_utf8(data)
        .map_err(|e| format!("File contains invalid UTF-8: {}", e))
}
