use actix_files::NamedFile;
use actix_web::{
    dev::Service,
    http::header::{HeaderValue, ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE},
    web, App, HttpRequest, HttpResponse, HttpServer, Responder, Scope,
};
use ayaka_runtime::log;
use std::{net::TcpListener, path::PathBuf, sync::OnceLock};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Runtime,
};

pub(crate) static ROOT_PATH: OnceLock<PathBuf> = OnceLock::new();

async fn fs_resolver(req: HttpRequest) -> impl Responder {
    let url = req.uri().path().strip_prefix("/fs/").unwrap_or_default();
    log::debug!("Acquiring FS {}", url);
    let path = ROOT_PATH.get().unwrap().join(url);
    if let Ok(file) = NamedFile::open_async(&path).await {
        file.into_response(&req)
    } else {
        HttpResponse::NotFound().finish()
    }
}

async fn resolver<R: Runtime>(app: AppHandle<R>, req: HttpRequest) -> impl Responder {
    let url = req.uri().path();
    log::debug!("Acquiring {}", url);
    if let Some(asset) = app.asset_resolver().get(url.to_string()) {
        HttpResponse::Ok()
            .append_header((CONTENT_TYPE, asset.mime_type.as_str()))
            .body(asset.bytes)
    } else {
        HttpResponse::NotFound().finish()
    }
}

pub fn init<R: Runtime>(listener: TcpListener) -> TauriPlugin<R> {
    Builder::new("asset_resolver")
        .setup(move |app| {
            let app = app.clone();
            std::thread::spawn(move || {
                actix_web::rt::System::new().block_on(async move {
                    HttpServer::new(move || {
                        let app = app.clone();
                        App::new()
                            .service(Scope::new("/fs").default_service(web::to(fs_resolver)))
                            .default_service(web::to(move |req| resolver(app.clone(), req)))
                            .wrap_fn(|req, srv| {
                                let fut = srv.call(req);
                                async {
                                    let mut res = fut.await?;
                                    res.headers_mut().insert(
                                        ACCESS_CONTROL_ALLOW_ORIGIN,
                                        HeaderValue::from_static("*"),
                                    );
                                    Ok(res)
                                }
                            })
                    })
                    .listen(listener)
                    .unwrap()
                    .run()
                    .await
                })
            });
            Ok(())
        })
        .build()
}