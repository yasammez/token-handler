use actix_web::{get, HttpResponse, Responder};
use actix_web::http::header;

#[get("/health")]
pub async fn health() -> impl Responder {
    HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, mime::APPLICATION_JSON))
        .body("\"up\"")
}