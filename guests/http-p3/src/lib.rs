wit_bindgen::generate!({
    path: "wit",
    world: "client",
});

struct Component;

impl Guest for Component {
    async fn fetch(authority: String) -> Result<String, String> {
        fetch(&authority)
            .await
            .map_err(|error| format!("{error:?}"))
    }
}

async fn fetch(authority: &str) -> Result<String, wasip3::http::types::ErrorCode> {
    use http_body_util::BodyExt;
    use wasip3::http::types::{Fields, Method, Request, Scheme};

    exercise_fields()?;
    let options = request_options()?;
    let fields = Fields::from_list(&[("x-client".to_owned(), b"middleware".to_vec())])
        .map_err(|_| invalid())?;
    let (_, trailers) = wasip3::wit_future::new(|| Ok(None));
    let (request, _) = Request::new(fields, None, trailers, Some(options));
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
    let request_options = request.get_options().ok_or_else(invalid)?;
    if request_options.get_connect_timeout() != Some(1_000_000_000)
        || request_options.get_first_byte_timeout() != Some(2_000_000_000)
        || request_options.get_between_bytes_timeout() != Some(3_000_000_000)
    {
        return Err(invalid());
    }
    drop(request_options);
    let request_summary = request_summary(&request)?;
    let response = wasip3::http::client::send(request).await?;
    let response_summary = response_summary(&response)?;
    let body = wasip3::http_compat::IncomingBody::new(response)?;
    let bytes = body.collect().await?.to_bytes();
    let body = String::from_utf8(bytes.to_vec()).map_err(|_| invalid())?;
    Ok(format!("{request_summary} -> {response_summary} | {body}"))
}

fn exercise_fields() -> Result<(), wasip3::http::types::ErrorCode> {
    use wasip3::http::types::Fields;

    let fields = Fields::new();
    fields
        .set("x-test", &[b"one".to_vec()])
        .map_err(|_| invalid())?;
    fields.append("x-test", b"two").map_err(|_| invalid())?;
    if fields.get("x-test").len() != 2 || !fields.has("x-test") || fields.copy_all().len() != 2 {
        return Err(invalid());
    }
    let cloned = fields.clone();
    let removed = fields.get_and_delete("x-test").map_err(|_| invalid())?;
    if removed.len() != 2 {
        return Err(invalid());
    }
    fields
        .set("x-delete", &[b"value".to_vec()])
        .map_err(|_| invalid())?;
    fields.delete("x-delete").map_err(|_| invalid())?;
    drop(cloned);
    Ok(())
}

fn request_options() -> Result<wasip3::http::types::RequestOptions, wasip3::http::types::ErrorCode>
{
    use wasip3::http::types::RequestOptions;

    let options = RequestOptions::new();
    options
        .set_connect_timeout(Some(1_000_000_000))
        .map_err(|_| invalid())?;
    options
        .set_first_byte_timeout(Some(2_000_000_000))
        .map_err(|_| invalid())?;
    options
        .set_between_bytes_timeout(Some(3_000_000_000))
        .map_err(|_| invalid())?;
    if options.get_connect_timeout() != Some(1_000_000_000)
        || options.get_first_byte_timeout() != Some(2_000_000_000)
        || options.get_between_bytes_timeout() != Some(3_000_000_000)
    {
        return Err(invalid());
    }
    drop(options.clone());
    Ok(options)
}

fn request_summary(
    request: &wasip3::http::types::Request,
) -> Result<String, wasip3::http::types::ErrorCode> {
    use wasip3::http::types::{Method, Scheme};

    if !matches!(request.get_method(), Method::Get)
        || !matches!(request.get_scheme(), Some(Scheme::Http))
        || request.get_path_with_query().as_deref() != Some("/message")
    {
        return Err(invalid());
    }
    let authority = request.get_authority().ok_or_else(invalid)?;
    let headers = request.get_headers();
    let values = headers.get("x-client");
    if !headers.has("x-client")
        || values.as_slice() != [b"middleware".as_slice()]
        || headers.copy_all() != [("x-client".to_owned(), b"middleware".to_vec())]
    {
        return Err(invalid());
    }
    Ok(format!(
        "GET http://{authority}/message x-client=middleware"
    ))
}

fn response_summary(
    response: &wasip3::http::types::Response,
) -> Result<String, wasip3::http::types::ErrorCode> {
    let headers = response.get_headers();
    let values = headers.get("x-server");
    if response.get_status_code() != 201
        || !headers.has("x-server")
        || values.as_slice() != [b"loopback".as_slice()]
        || !headers
            .copy_all()
            .iter()
            .any(|(name, value)| name == "x-server" && value == b"loopback")
    {
        return Err(invalid());
    }
    Ok("201 x-server=loopback".to_owned())
}

fn invalid() -> wasip3::http::types::ErrorCode {
    wasip3::http::types::ErrorCode::InternalError(Some("invalid HTTP state".to_owned()))
}

export!(Component);
