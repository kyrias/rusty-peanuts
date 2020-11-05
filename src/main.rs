#![feature(termination_trait_lib)]

pub enum Exit<T> {
    Ok,
    Err(T),
}

impl<T: Into<i32> + std::fmt::Display> std::process::Termination for Exit<T> {
    fn report(self) -> i32 {
        match self {
            Exit::Ok => 0,
            Exit::Err(err) => {
                eprintln!("Error: {}", err);
                err.into()
            },
        }
    }
}

#[async_std::main]
async fn main() -> Exit<rusty_peanuts::Error> {
    match rusty_peanuts::main().await {
        Ok(_) => Exit::Ok,
        Err(err) => Exit::Err(err),
    }
}
