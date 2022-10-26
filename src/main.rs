#[macro_use]
extern crate serde;

use std::io::Write;
use ansi_term::Style;

mod barcode;
mod db;
mod products;

const FORBIDDEN_USERS: [&str; 11] = ["help", "?", "reload", "products", "adduser", "deposit",
    "users", "deposits", "purchases", "abort", "cancel"];

pub struct Cart {
    products: Vec<products::Product>
}

impl Cart {
    fn new() -> Self {
        Self {
            products: Vec::new()
        }
    }

    fn total(&self) -> u32 {
        self.products.iter().map(|p| p.price).sum()
    }

    fn print(&self) {
        println!("{}", Style::new().bold().underline().paint("Current cart"));
        for product in &self.products {
            println!("- {} (£{:.2})", product.name, product.price as f64 / 100.0);
        }
        println!("Total: £{:.2}", self.total() as f64 / 100.0);
    }
}

fn main() -> std::io::Result<()> {
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
    let mut cart: Option<Cart> = None;

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    loop {
        let mut buffer = String::new();
        if cart.is_none() {
            print!("{}", Style::new().bold().paint("57Bank> "));
        } else {
            print!("{}", Style::new().bold().paint("57Bank (cart in progress)> "));
        }
        stdout.flush()?;
        stdin.read_line(&mut buffer)?;
        buffer = buffer.trim().to_string();

        if !buffer.is_empty() {
            let mut args = buffer.split_whitespace();
            let command = args.next().unwrap();
            let args = args.collect::<Vec<_>>();

            match (barcode::Barcode::try_parse(command), args.is_empty()) {
                (Some(barcode), true) => {
                    if !barcode.check_digit() {
                        println!("Invalid barcode")
                    } else if let Some(product) = product_store.get(&barcode) {
                        println!("Adding {} to cart", product.name);
                        if cart.is_none() {
                            cart = Some(Cart::new());
                        }

                        let c_cart = cart.as_mut().unwrap();
                        c_cart.products.push(product.clone());
                        c_cart.print();
                    } else {
                        println!("Unknown product");
                    }
                },
                _ => match (db.get_user(command), args.is_empty(), cart.is_some()) {
                    (Some(user), true, false) => {
                        println!("{}", Style::new().underline().paint(format!("User {}", user.0.id)));
                        println!("Balance: {}", user.0.disp_balance());
                        println!("{}", Style::new().underline().paint("Recent transactions"));
                        for t in user.1.iter().rev().take(10) {
                            match &t.transaction {
                                db::TransactionType::Deposit {
                                    amount, method
                                } => println!("Deposit £{:.2} ({})", *amount as f64 / 100.0, match method {
                                    db::DepositMethod::Cash => "cash",
                                    db::DepositMethod::BankTransfer => "bank transfer"
                                }),
                                db::TransactionType::Purchase {
                                    total, products
                                } => {
                                    println!("Purchase (total £{:.2})", *total as f64 / 100.0);
                                    for p in products {
                                        println!("- {} (£{:.2})", p.name, p.price as f64 / 100.0);
                                    }
                                }
                            }
                            println!("Timestamp: {}", t.timestamp);
                            println!()
                        }
                    },
                    (Some(user), true, true) => {
                        match db.apply_cart_to_user(&user.0.id, cart.as_ref().unwrap()) {
                            Ok(user) => {
                                println!("Charged to user {}", Style::new().bold().paint(&user.id));
                                println!("New balance: {}", user.disp_balance());
                                cart = None;
                            },
                            Err(e) => {
                                println!("Error, unable to charge user: {}", e);
                            }
                        }
                    }
                    _ => match command {
                        "help" | "?" => help(),
                        "reload" => reload(&mut product_store),
                        "products" => products(&product_store),
                        "adduser" => adduser(&db, &args),
                        "deposit" => deposit(&db, &args),
                        "users" => users(&db),
                        "deposits" => deposits(&db),
                        "purchases" => purchases(&db),
                        "abort" | "cancel" => {
                            cart = None;
                            println!("Cart abandoned");
                        },
                        _ => println!("\x07Unknown command: {}", command),
                    }
                }
            }
        }
    }
}

fn help() {
    println!("{}", Style::new().bold().underline().paint("--- 57North Snack Bank ---"));
    println!();
    println!("{}", Style::new().underline().paint("Buying something"));
    println!("Scan the barcode on the item to add to cart, complete transaction by typing in your account ID.");
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
    println!("{}", Style::new().underline().paint("Other commands (generally internal use only)"));
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
        println!("{} - £{:.2} ({})", product.name, product.price as f64 / 100.0, barcode);
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
        },
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
                break (amount * 100.0) as u32
            },
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
            break db::DepositMethod::BankTransfer;
        } else {
            println!("Invalid method")
        }
    };

    match db.deposit_user(args[0], amount, method) {
        Ok(user) => {
            println!("Deposited applied to user {}", user.id);
            println!("New balance: {}", user.disp_balance());
            println!("{}", Style::new().bold().paint("Please transfer money for this deposit / put it in the cash box"));
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
        println!("{} - £{:.2}", user.id, user.balance as f64 / 100.0);
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
    }.iter().filter(|t| matches!(t.transaction, db::TransactionType::Deposit { .. })).rev().take(10) {
        match &t.transaction {
            db::TransactionType::Deposit {
                amount, method
            } => {
                println!("Deposit £{:.2} ({}), by {} at {}", *amount as f64 / 100.0, match method {
                    db::DepositMethod::Cash => "cash",
                    db::DepositMethod::BankTransfer => "bank transfer"
                }, t.actor, t.timestamp);
            },
            _ => unreachable!()
        }
    }
}

fn purchases(db: &db::DB) {
    println!("{}", Style::new().underline().paint("Recent transactions"));

    for t in match db.transactions() {
        Ok(u) => u,
        Err(e) => {
            println!("Error, unable to list transactions: {}", e);
            return;
        }
    }.iter().filter(|t| matches!(t.transaction, db::TransactionType::Purchase { .. })).rev().take(10) {
        match &t.transaction {
            db::TransactionType::Purchase {
                products, total
            } => {
                println!("Purchase (total £{:.2}) by {} at {}", *total as f64 / 100.0, t.actor, t.timestamp);
                for p in products {
                    println!("- {} (£{:.2})", p.name, p.price as f64 / 100.0);
                }
            },
            _ => unreachable!()
        }
    }
}