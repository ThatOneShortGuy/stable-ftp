use chrono::prelude::*;
use std::fs;
use std::io::Write;

fn write_to_logs(message: impl AsRef<str>) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open("Logs.txt")
        .unwrap();

    if let Err(e) = writeln!(file, "{}", message.as_ref()) {
        eprintln!("Couldn't write to file: {e}");
    }
}

fn format_message(message: impl AsRef<str>, message_type: impl AsRef<str>) -> String {
    let utc = Utc::now();
    let date_time = DateTime::<Local>::from(utc);
    let message = format!(
        "{date_time} {} ::: {}",
        message_type.as_ref(),
        message.as_ref()
    );
    message
}

#[allow(dead_code)]
pub fn error(message: impl AsRef<str>) -> ! {
    let message = format_message(message, "ERROR");

    eprintln!("\x1b[31m{message}\x1b[0m");
    write_to_logs(&message);
    eprintln!("You can find this error in Logs.txt");
    panic!()
}

#[allow(dead_code)]
pub fn warning(message: impl AsRef<str>) {
    let message = format_message(message, "WARNING");

    eprintln!("\x1b[33m{message}\x1b[0m\n");
    write_to_logs(&message);
}

#[allow(dead_code)]
pub fn info(message: impl AsRef<str>) {
    let message = format_message(message, "INFO");

    eprintln!("{message}");
    write_to_logs(&message);
}

#[cfg(test)]
mod tests {
    #[test]
    fn t1() {
        super::info("An info message");
        super::warning("A Warning");
    }

    #[test]
    #[should_panic]
    fn t2() {
        super::error("An error")
    }
}
