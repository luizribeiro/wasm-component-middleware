wit_bindgen::generate!({
    path: "wit",
    world: "client",
});

use wasip2::http::outgoing_handler;
use wasip2::http::types::{
    Fields, IncomingBody, Method, OutgoingBody, OutgoingRequest, RequestOptions, Scheme,
};
use wasip2::io::poll::poll;
use wasip2::io::streams::StreamError;

struct Component;

impl Guest for Component {
    fn fetch(authority: String) -> Result<String, String> {
        fetch(&authority).map_err(|error| format!("{error:?}"))
    }
}

fn fetch(authority: &str) -> Result<String, wasip2::http::types::ErrorCode> {
    exercise_fields()?;
    let options = request_options()?;
    let request = OutgoingRequest::new(
        Fields::from_list(&[("x-client".to_owned(), b"middleware".to_vec())])
            .map_err(|_| invalid())?,
    );
    request.set_method(&Method::Get).map_err(|()| invalid())?;
    request
        .set_scheme(Some(&Scheme::Http))
        .map_err(|()| invalid())?;
    request
        .set_authority(Some(authority))
        .map_err(|()| invalid())?;
    request
        .set_path_with_query(Some("/message"))
        .map_err(|()| invalid())?;
    let outgoing_body = request.body().map_err(|()| invalid())?;
    let stream = outgoing_body.write().map_err(|()| invalid())?;
    stream
        .blocking_write_and_flush(&[])
        .map_err(|_| invalid())?;
    drop(stream);
    OutgoingBody::finish(outgoing_body, None)?;
    let request_summary = request_summary(&request)?;
    let response = outgoing_handler::handle(request, Some(options))?;
    poll(&[&response.subscribe()]);
    let response = response
        .get()
        .ok_or_else(invalid)?
        .map_err(|()| invalid())??;
    let response_summary = response_summary(&response)?;
    let body = response.consume().map_err(|()| invalid())?;
    let stream = body.stream().map_err(|()| invalid())?;
    let mut bytes = Vec::new();
    loop {
        match stream.blocking_read(8192) {
            Ok(chunk) => bytes.extend(chunk),
            Err(StreamError::Closed) => break,
            Err(StreamError::LastOperationFailed(_)) => return Err(invalid()),
        }
    }
    drop(stream);
    let trailers = IncomingBody::finish(body);
    poll(&[&trailers.subscribe()]);
    let trailers = trailers
        .get()
        .ok_or_else(invalid)?
        .map_err(|()| invalid())??;
    drop(trailers);
    let body = String::from_utf8(bytes).map_err(|_| invalid())?;
    Ok(format!("{request_summary} -> {response_summary} | {body}"))
}

fn exercise_fields() -> Result<(), wasip2::http::types::ErrorCode> {
    let fields = Fields::new();
    fields
        .set("x-test", &[b"one".to_vec()])
        .map_err(|_| invalid())?;
    fields.append("x-test", b"two").map_err(|_| invalid())?;
    if fields.get("x-test").len() != 2 || !fields.has("x-test") || fields.entries().len() != 2 {
        return Err(invalid());
    }
    let cloned = fields.clone();
    fields.delete("x-test").map_err(|_| invalid())?;
    drop(cloned);
    Ok(())
}

fn request_options() -> Result<RequestOptions, wasip2::http::types::ErrorCode> {
    let options = RequestOptions::new();
    options
        .set_connect_timeout(Some(1_000_000_000))
        .map_err(|()| invalid())?;
    options
        .set_first_byte_timeout(Some(2_000_000_000))
        .map_err(|()| invalid())?;
    options
        .set_between_bytes_timeout(Some(3_000_000_000))
        .map_err(|()| invalid())?;
    if options.connect_timeout() != Some(1_000_000_000)
        || options.first_byte_timeout() != Some(2_000_000_000)
        || options.between_bytes_timeout() != Some(3_000_000_000)
    {
        return Err(invalid());
    }
    Ok(options)
}

fn request_summary(request: &OutgoingRequest) -> Result<String, wasip2::http::types::ErrorCode> {
    if !matches!(request.method(), Method::Get)
        || !matches!(request.scheme(), Some(Scheme::Http))
        || request.path_with_query().as_deref() != Some("/message")
    {
        return Err(invalid());
    }
    let authority = request.authority().ok_or_else(invalid)?;
    let headers = request.headers();
    let values = headers.get("x-client");
    if !headers.has("x-client")
        || values.as_slice() != [b"middleware".as_slice()]
        || headers.entries() != [("x-client".to_owned(), b"middleware".to_vec())]
    {
        return Err(invalid());
    }
    Ok(format!(
        "GET http://{authority}/message x-client=middleware"
    ))
}

fn response_summary(
    response: &wasip2::http::types::IncomingResponse,
) -> Result<String, wasip2::http::types::ErrorCode> {
    let headers = response.headers();
    let values = headers.get("x-server");
    if response.status() != 201
        || !headers.has("x-server")
        || values.as_slice() != [b"loopback".as_slice()]
        || !headers
            .entries()
            .iter()
            .any(|(name, value)| name == "x-server" && value == b"loopback")
    {
        return Err(invalid());
    }
    Ok("201 x-server=loopback".to_owned())
}

fn invalid() -> wasip2::http::types::ErrorCode {
    wasip2::http::types::ErrorCode::InternalError(Some("invalid HTTP state".to_owned()))
}

export!(Component);
