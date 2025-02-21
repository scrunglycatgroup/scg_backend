use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct GenDocInput<'r> {
    model: &'r str,
    code: &'r str,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct GenDocOutput<'r> {
    code: &'r str,
}

#[post("/documentation", format = "application/json", data = "<input>")]
pub fn gen_docs(input: Json<GenDocInput<'_>>) -> Json<GenDocOutput<'_>> {
    todo!()
}
