extern crate rocket;

mod generate;

#[get("/")]
pub fn index() -> &'static str {
    "Hello world!"
}

#[rocket::main]
pub async fn rocket() -> Result<(), rocket::Error> {
    rocket::build()
        .mount("/", routes![index])
        .mount(
            "/generate",
            routes![
                generate::gen_documentation::gen_docs,
                generate::gen_horror::generate_horror
            ],
        )
        .launch()
        .await?;
    Ok(())
}
