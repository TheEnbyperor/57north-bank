#![feature(iterator_try_collect)]
#![feature(never_type)]

#[macro_use]
extern crate serde;

use ansi_term::{Color, Style};
use completion::Hintererer;
use db::{User, Transaction};
use nfc1::target_info::{Iso14443a, TargetInfo};
use rustyline::{error::ReadlineError, Editor};
use std::{
    future::Future,
    io::{stdout, Stdout, Write},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::{
    select,
    sync::{
        mpsc::{self, Receiver},
        Mutex,
    },
};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + Sync>>;

mod barcode;
mod completion;
mod db;
mod products;

const FORBIDDEN_USERS: [&str; 16] = [
    "help",
    "?",
    "hilfe",
    "reload",
    "products",
    "adduser",
    "deposit",
    "users",
    "deposits",
    "purchases",
    "abort",
    "cancel",
    "cash",
    "clear",
    "regcard",
    "delcard",
];
const MONZO_USERNAME: &str = "davidhibberd";

pub struct Cart {
    products: Vec<products::Product>,
}

impl Cart {
    fn new() -> Self {
        Self {
            products: Vec::new(),
        }
    }

    fn total(&self) -> u32 {
        self.products.iter().map(|p| p.price).sum()
    }

    fn disp_total(&self) -> String {
        format!("£{:.2}", self.total() as f64 / 100.0)
    }

    fn print(&self) {
        println!("{}", Style::new().bold().underline().paint("Current cart"));
        for product in &self.products {
            println!("- {} ({})", product.name, product.disp_price());
        }
        println!("Total: {}", self.disp_total());
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let db = match db::DB::load() {
        Ok(d) => d,
        Err(e) => {
            println!("Error, unable to open database: {}", e);
            return Ok(());
        }
    };
    let mut product_store = match products::read_products() {
        Ok(p) => p,
        Err(e) => {
            println!("Error, unable to load products: {}", e);
            return Ok(());
        }
    };
    let cart: Arc<Mutex<Option<Cart>>> = Arc::new(Mutex::new(None));

    let stdin: Arc<Mutex<Editor<Hintererer, rustyline::history::FileHistory>>> =
        Arc::new(Mutex::new(Editor::new()?));
    let mut stdin_lock = stdin.lock().await;
    stdin_lock.set_helper(Some(Hintererer::new()));
    if stdin_lock.load_history("data/history").is_err() {
        println!("No previous history.");
    }
    drop(stdin_lock);

    let mut stdout = stdout();
    clear(&mut stdout);

    let (card_tx, mut card_rx_handle) = mpsc::channel::<Vec<u8>>(1);
    let stop_reader = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_reader);

    let cards_joinhandle = tokio::task::spawn_blocking::<
        _,
        Result<(), Box<dyn std::error::Error + Send + Sync>>,
    >(move || {
        let mut context = nfc1::Context::new().unwrap();
        let mut device = context.open().unwrap();

        device.initiator_init().unwrap();

        loop {
            if stop_clone.load(Ordering::SeqCst) {
                break;
            }
            match device.initiator_select_passive_target(&nfc1::Modulation {
                modulation_type: nfc1::ModulationType::Iso14443a,
                baud_rate: nfc1::BaudRate::Baud106,
            }) {
                Ok(target) => {
                    if let TargetInfo::Iso14443a(Iso14443a { uid, uid_len, .. }) =
                        target.target_info
                    {
                        card_tx.blocking_send(uid[..uid_len].to_vec())?;
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(())
    });

    let (stdout_tx, mut stdout_rx_handle) = mpsc::channel::<StdoutMsg>(1);

    let stdin_c = Arc::clone(&stdin);
    let cart_c = Arc::clone(&cart);
    let stop_clone = Arc::clone(&stop_reader);

    let stdout_joinhandle = tokio::spawn::<
        BoxFuture<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
    >(Box::pin(async move {
        loop {
            let permit = stdout_tx.reserve().await?;

            let buffer = if cart_c.lock().await.is_none() {
                stdin_c
                    .lock()
                    .await
                    .readline(&format!("{} ", Style::new().bold().paint("57Bank>")))
            } else {
                stdin_c.lock().await.readline(&format!(
                    "{}{}{}",
                    Style::new().bold().paint("57Bank"),
                    Style::new()
                        .bold()
                        .on(Color::Yellow)
                        .paint("(cart in progress)"),
                    Style::new().bold().paint("> ")
                ))
            };

            let buffer = match buffer {
                Ok(t) => StdoutMsg::Text(t),
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => {
                    println!("{}", Style::new().bold().fg(Color::Red).paint("EXITING..."));
                    StdoutMsg::Signal(Signal::Kill);
                    stop_clone.store(true, Ordering::SeqCst);
                    return Ok(());
                }
                Err(_error) => StdoutMsg::Signal(Signal::Kill),
            };

            permit.send(buffer);
        }
    }));

    loop {
        // let buffer = if cart.is_none() {
        //     stdin.readline(&format!("{} ", Style::new().bold().paint("57Bank>")))
        // } else {
        //     stdin.readline(&format!(
        //         "{}{}{}",
        //         Style::new().bold().paint("57Bank"),
        //         Style::new()
        //             .bold()
        //             .on(Color::Yellow)
        //             .paint("(cart in progress)"),
        //         Style::new().bold().paint("> ")
        //     ))
        // };

        // let buffer = match buffer {
        //     Ok(t) => t,
        //     Err(ReadlineError::Interrupted | ReadlineError::Eof) => {
        //         println!("{}", Style::new().bold().fg(Color::Red).paint("EXITING..."));
        //         break;
        //     }
        //     Err(_error) => break,
        // };

        let buffer = select! {
            msg = stdout_rx_handle.recv() => {
                match msg {
                    Some(StdoutMsg::Text(t)) => t,
                    Some(StdoutMsg::Signal(_)) => {
                        stdout_rx_handle.close();
                        break
                    },
                    None => continue,
                }
            },
            uid = card_rx_handle.recv() => {
                if let Some(card_id) = uid {
                    let card_id_str = card_id.into_iter().map(|b| b.to_string()).collect::<String>();
                    let user = match db.get_user_by_card(&card_id_str) {
                        Some(u) => u,
                        None => continue,
                    };
                    let cart_c = Arc::clone(&cart);
                    let cart_unlocked = cart_c.lock().await;

                    if cart_unlocked.is_none() {
                        println!();
                        user_info(user);
                        continue;
                    }
                    drop(cart_unlocked);

                    println!();
                    complete_cart(&db, user, Arc::clone(&cart)).await;
                }
                continue;
            }
        };

        if !buffer.is_empty() {
            let mut stdin_l = stdin.lock().await;
            stdin_l.add_history_entry(&buffer)?;
            drop(stdin_l);
            let mut args = buffer.split_whitespace();
            let command = args.next().unwrap();
            let args = args.collect::<Vec<_>>();

            match command {
                "hilfe" | "help" | "?" => help(),
                "clear" => clear(&mut stdout),
                "reload" => reload(&mut product_store),
                "products" => products(&product_store),
                "adduser" => adduser(&db, &args),
                "regcard" => register_card(&args, &db, &mut card_rx_handle).await,
                // "delcard" => delete_card(&args, &db, &mut card_rx_handle),
                "deposit" => deposit(&db, &args),
                "users" => users(&db),
                "deposits" => deposits(&db),
                "purchases" => purchases(&db),
                "abort" | "cancel" => {
                    *cart.lock().await = None;
                    println!("Cart abandoned");
                }
                "cash" => {
                    if cart.lock().await.is_some() {
                        let mut cart = cart.lock().await;
                        let c_cart = cart.as_ref().unwrap();
                        match db.apply_cart_to_cash(c_cart) {
                            Ok(()) => {
                                println!(
                                    "{}",
                                    Style::new().bold().paint(format!(
                                        "Please put {} in the cash box",
                                        c_cart.disp_total()
                                    ))
                                );
                                *cart = None;
                            }
                            Err(e) => {
                                println!("Error, unable to charge: {}", e);
                            }
                        }
                    } else {
                        println!("Nothing in cart")
                    }
                }
                _ => match (barcode::Barcode::try_parse(command), args.is_empty()) {
                    (Some(barcode), true) => {
                        if !barcode.check_digit() {
                            println!("Invalid barcode")
                        } else if let Some(product) = product_store.get(&barcode) {
                            println!("Adding {} to cart", product.name);
                            if cart.lock().await.is_none() {
                                *cart.lock().await = Some(Cart::new());
                            }

                            let mut cart = cart.lock().await;
                            let c_cart = cart.as_mut().unwrap();
                            c_cart.products.push(product.clone());
                            c_cart.print();
                        } else {
                            println!("Unknown product");
                        }
                    }
                    _ => match (
                        db.get_user(command),
                        args.is_empty(),
                        cart.lock().await.is_some(),
                    ) {
                        (Some(user), true, false) => {
                            println!(
                                "{}",
                                Style::new()
                                    .underline()
                                    .paint(format!("User {}", user.0.id))
                            );
                            println!("Balance: {}", user.0.disp_balance());
                            println!("{}", Style::new().underline().paint("Recent transactions"));
                            for t in user.1.iter().rev().take(10) {
                                match &t.transaction {
                                    db::TransactionType::Deposit { amount, method } => println!(
                                        "Deposit £{:.2} ({})",
                                        *amount as f64 / 100.0,
                                        match method {
                                            db::DepositMethod::Cash => "cash",
                                            db::DepositMethod::BankTransfer => "bank transfer",
                                        }
                                    ),
                                    db::TransactionType::Purchase { total, products } => {
                                        println!("Purchase (total £{:.2})", *total as f64 / 100.0);
                                        for p in products {
                                            println!("- {} ({})", p.name, p.disp_price());
                                        }
                                    }
                                }
                                println!("Timestamp: {}", t.timestamp);
                                println!()
                            }
                        }
                        (Some(user), true, true) => {
                            let cart = Arc::clone(&cart);
                            complete_cart(&db, user, cart).await
                        }
                        _ => println!("\x07Unknown command: {}", command),
                    },
                },
            }
        }
    }

    stdin.lock().await.save_history("data/history")?;
    clear(&mut stdout);

    cards_joinhandle.abort();
    // stdout_joinhandle.abort();

    Ok(())
}

async fn complete_cart(db: &db::DB, user: (User, Vec<Transaction>), cart: Arc<Mutex<Option<Cart>>>) {
    let cart_unlocked = cart.lock().await;
    match db.apply_cart_to_user(&user.0.id, cart_unlocked.as_ref().unwrap()) {
        Ok(user) => {
            println!("Charged to user {}", Style::new().bold().paint(&user.id));
            println!("New balance: {}", user.disp_balance());
            *cart.lock().await = None;
            drop(cart_unlocked);
        }
        Err(e) => {
            println!("Error, unable to charge user: {}", e);
        }
    }
}

fn user_info(user: (User, Vec<Transaction>)) {
    println!(
        "{}",
        Style::new()
            .underline()
            .paint(format!("User {}", user.0.id))
    );
    println!("Balance: {}", user.0.disp_balance());
    println!("{}", Style::new().underline().paint("Recent transactions"));
    for t in user.1.iter().rev().take(10) {
        match &t.transaction {
            db::TransactionType::Deposit { amount, method } => println!(
                "Deposit £{:.2} ({})",
                *amount as f64 / 100.0,
                match method {
                    db::DepositMethod::Cash => "cash",
                    db::DepositMethod::BankTransfer => "bank transfer",
                }
            ),
            db::TransactionType::Purchase { total, products } => {
                println!("Purchase (total £{:.2})", *total as f64 / 100.0);
                for p in products {
                    println!("- {} ({})", p.name, p.disp_price());
                }
            }
        }
        println!("Timestamp: {}", t.timestamp);
        println!()
    }
}

#[derive(Debug)]
pub enum StdoutMsg {
    Text(String),
    Signal(Signal),
}

#[derive(Debug)]
pub enum Signal {
    Kill,
    Eof,
}

fn clear(stdout: &mut Stdout) {
    print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
    stdout.flush().unwrap();
}

fn help() {
    println!(
        "{}",
        Style::new()
            .bold()
            .underline()
            .paint("--- 57North Snack Bank ---")
    );
    println!();
    println!("{}", Style::new().underline().paint("Buying something"));
    println!("Scan the barcode on the item to add to cart, complete transaction by typing in your account ID.");
    println!("Alternatively type in cash to pay with cash directly into the box.");
    println!("Type 'abort' or 'cancel' at any time to cancel the cart.");
    println!();
    println!("{}", Style::new().underline().paint("Adding money"));
    println!("Type 'deposit <id>' with your account ID to start the deposit process.");
    println!();
    println!("{}", Style::new().underline().paint("New users"));
    println!("Type 'adduser <id>' with your desired account ID to create an new account.");
    println!();
    println!("{}", Style::new().underline().paint("View products"));
    println!("Type 'products' to view a product listing and prices.");
    println!();
    println!("{}", Style::new().underline().paint("Check balance"));
    println!("Type your user ID to view balance and recent transactions.");
    println!();
    println!(
        "{}",
        Style::new().underline().paint("Adding and removing cards")
    );
    println!("Type 'regcard <id> [name]' with your desired account ID and optionally the name of the card to start the card registration process");
    println!("Type 'delcard <id> [name]' with your desired account ID and optionally the name of the card to start the card deletion process");
    println!();
    println!(
        "{}",
        Style::new()
            .underline()
            .paint("Other commands (generally internal use only)")
    );
    println!("- reload");
    println!("- users");
    println!("- deposits");
    println!("- purchases");
}

fn reload(products: &mut products::Products) {
    *products = match products::read_products() {
        Ok(p) => p,
        Err(e) => {
            println!("Error, unable to load products: {}", e);
            return;
        }
    };
}

fn products(products: &products::Products) {
    println!("{}", Style::new().underline().paint("Product listing"));
    for (barcode, product) in products {
        println!("{} - {} ({})", product.name, product.disp_price(), barcode);
    }
}

fn adduser(db: &db::DB, args: &[&str]) {
    if args.len() < 1 {
        println!("Usage: adduser <id>");
        return;
    }

    if FORBIDDEN_USERS.contains(&args[0]) {
        println!("Error, user ID is forbidden");
        return;
    }

    match db.add_user(args[0]) {
        Ok(_) => {
            println!("User {} added", args[0]);
        }
        Err(e) => {
            println!("Error, unable to add user: {}", e);
        }
    }
}

fn deposit(db: &db::DB, args: &[&str]) {
    if args.len() < 1 {
        println!("Usage: deposit <id>");
        return;
    }

    let amount = loop {
        print!("Amount to deposit ('abort' to cancel): ");
        std::io::stdout().flush().unwrap();

        let mut buffer = String::new();
        std::io::stdin().read_line(&mut buffer).unwrap();
        let buffer = buffer.trim().to_string();

        if buffer == "abort" {
            return;
        }

        match buffer.parse::<f64>() {
            Ok(amount) => {
                if amount <= 0.0 {
                    println!("Invalid amount");
                    continue;
                }
                break (amount * 100.0) as u32;
            }
            Err(_) => println!("Invalid amount"),
        }
    };

    let method = loop {
        print!("Deposit method (cash / bank; 'abort' to cancel): ");
        std::io::stdout().flush().unwrap();

        let mut buffer = String::new();
        std::io::stdin().read_line(&mut buffer).unwrap();
        let buffer = buffer.trim().to_string();

        if buffer == "abort" {
            return;
        } else if buffer == "cash" {
            break db::DepositMethod::Cash;
        } else if buffer == "bank" {
            #[derive(Debug, Serialize, Deserialize, Clone)]
            pub struct CardUID {}
            break db::DepositMethod::BankTransfer;
        } else {
            println!("Invalid method")
        }
    };

    match db.deposit_user(args[0], amount, method) {
        Ok(user) => {
            println!("Deposited applied to user {}", user.id);
            println!("New balance: {}", user.disp_balance());
            println!(
                "{}",
                Style::new()
                    .bold()
                    .paint("Please transfer money for this deposit / put it in the cash box")
            );
            if method == db::DepositMethod::BankTransfer {
                let qr_code = qrcode_generator::to_matrix(
                    format!(
                        "https://monzo.me/{}/{:.2}?d=57Bank",
                        MONZO_USERNAME,
                        amount as f64 / 100.0
                    ),
                    qrcode_generator::QrCodeEcc::Low,
                )
                .unwrap();
                for _ in 0..2 {
                    for _ in 0..qr_code.len() + 4 {
                        print!("\u{2588}\u{2588}");
                    }
                    println!();
                }
                for row in &qr_code {
                    print!("\u{2588}\u{2588}\u{2588}\u{2588}");
                    for col in row {
                        if *col {
                            print!("  ");
                        } else {
                            print!("\u{2588}\u{2588}");
                        }
                    }
                    println!("\u{2588}\u{2588}\u{2588}\u{2588}");
                }
                for _ in 0..2 {
                    for _ in 0..qr_code.len() + 4 {
                        print!("\u{2588}\u{2588}");
                    }
                    println!();
                }
            }
        }
        Err(e) => {
            println!("Error, unable to deposit: {}", e);
            return;
        }
    }
}

fn users(db: &db::DB) {
    println!("{}", Style::new().underline().paint("Users"));

    for user in match db.users() {
        Ok(u) => u,
        Err(e) => {
            println!("Error, unable to list users: {}", e);
            return;
        }
    } {
        println!("{} - {}", user.id, user.disp_balance());
    }
}

fn deposits(db: &db::DB) {
    println!("{}", Style::new().underline().paint("Recent deposits"));

    for t in match db.transactions() {
        Ok(u) => u,
        Err(e) => {
            println!("Error, unable to list transactions: {}", e);
            return;
        }
    }
    .iter()
    .filter(|t| matches!(t.transaction, db::TransactionType::Deposit { .. }))
    .rev()
    .take(10)
    {
        match &t.transaction {
            db::TransactionType::Deposit { amount, method } => {
                println!(
                    "Deposit £{:.2} ({}), by {} at {}",
                    *amount as f64 / 100.0,
                    match method {
                        db::DepositMethod::Cash => "cash",
                        db::DepositMethod::BankTransfer => "bank transfer",
                    },
                    t.actor,
                    t.timestamp
                );
            }
            _ => unreachable!(),
        }
    }
}

fn purchases(db: &db::DB) {
    if db.transactions().is_ok_and(|tx| tx.is_empty()) {
        println!(
            "{}",
            Style::new()
                .underline()
                .fg(Color::Red)
                .paint("No recent transactions")
        );
        return;
    }
    println!("{}", Style::new().underline().paint("Recent transactions"));

    for t in match db.transactions() {
        Ok(u) => u,
        Err(e) => {
            println!("Error, unable to list transactions: {}", e);
            return;
        }
    }
    .iter()
    .filter(|t| matches!(t.transaction, db::TransactionType::Purchase { .. }))
    .rev()
    .take(10)
    {
        match &t.transaction {
            db::TransactionType::Purchase { products, total } => {
                println!(
                    "Purchase (total £{:.2}) by {} at {}",
                    *total as f64 / 100.0,
                    t.actor,
                    t.timestamp
                );
                for p in products {
                    println!("- {} ({})", p.name, p.disp_price());
                }
            }
            _ => unreachable!(),
        }
    }
}

async fn register_card(args: &[&str], db: &db::DB, reader: &mut Receiver<Vec<u8>>) {
    if args.is_empty() {
        println!("Usage: regcard <id> [card name]");
        return;
    }
    let id = args[0];

    let name = if args.len() > 1 {
        Some(args[1..].join(" "))
    } else {
        None
    };

    println!("Please touch the card to the reader");
    let mut uids = Vec::new();
    for _ in 0..2 {
        uids.push(reader.recv().await.unwrap());
    }

    println!("Validating card...");

    if uids[0] != uids[1] {
        println!("Your card does not have a static UID, sorry :(");
        return;
    }

    match db.add_card_to_user(
        id,
        name,
        uids[0].iter().map(|b| b.to_string()).collect::<String>(),
    ) {
        Ok((name, uid)) => {
            println!("A card with ID {uid} and name '{name}' has been associated with your user")
        }
        Err(e) => println!("Error, failed to write the card information to your user: {e}"),
    }
}

// fn delete_card(args: &[&str], db: &db::DB, reader: &mut Reader) {
//     if args.is_empty() {
//         println!("Usage: delcard <id> [card name]");
//         return;
//     }
//     let id = args[0];
//     let name = if args.len() > 1 {
//         Some(args[1..].join(" "))
//     } else {
//         None
//     };

//     if name.is_none() {
//         println!("Please present the card you would like to delete");

//         let mut card = reader.connect().unwrap();
//         let tx = card.transaction_blocking(reader).unwrap();
//         let uid = tx
//             .get_uid()
//             .iter()
//             .map(|b| b.to_string())
//             .collect::<String>();

//         match db.delete_card(id, CardNameOrID::ID(uid.clone())) {
//             Ok(_) => println!("Successfully removed the card '{uid}' from the database"),
//             Err(e) => println!("Error, failed to remove the card: {e}"),
//         }
//     } else {
//         let name = name.unwrap();
//         match db.delete_card(id, CardNameOrID::Name(name.clone())) {
//             Ok(_) => println!("Successfully removed the card '{name}' from the database"),
//             Err(e) => println!("Error, failed to remove the card: {e}"),
//         }
//     }
// }
