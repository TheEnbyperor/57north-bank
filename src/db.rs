use std::fmt::Formatter;
use chrono::prelude::*;
use ansi_term::Style;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InnerDB {
    pub users: std::collections::HashMap<String, User>,
    pub transactions: Vec<Transaction>
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: String,
    pub balance: i32,
}

impl User {
    pub fn disp_balance(&self) -> String {
        if self.balance < 0 {
            Style::new().fg(ansi_term::Color::Red).paint(format!("-£{:.2}", -self.balance as f64 / 100.0)).to_string()
        } else {
           format!("£{:.2}", self.balance as f64 / 100.0)
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Transaction {
    pub timestamp: DateTime<Utc>,
    pub actor: TransactionActor,
    pub transaction: TransactionType,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TransactionActor {
    User(String),
    Cash
}

impl std::fmt::Display for TransactionActor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User(id) => write!(f, "user {}", id),
            Self::Cash => write!(f, "cash")
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TransactionType {
    Purchase {
        products: Vec<crate::products::Product>,
        total: u32,
    },
    Deposit {
        amount: u32,
        method: DepositMethod,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum DepositMethod {
    Cash,
    BankTransfer
}

type DBStore = rustbreak::PathDatabase<InnerDB, rustbreak::deser::Ron>;

pub struct DB(DBStore);

impl DB {
    pub fn load() -> Result<DB, String> {
        Ok(DB(DBStore::load_from_path_or_else("./data/db".into(), || {
            InnerDB {
                users: std::collections::HashMap::new(),
                transactions: Vec::new(),
            }
        }).map_err(|e| format!("{:?}", e))?))
    }

    pub fn get_user(&self, id: &str) -> Option<(User, Vec<Transaction>)> {
        let mut data = self.0.get_data(true).ok()?;
        let u = data.users.remove(id)?;
        let t = data.transactions.iter().filter(|t| match &t.actor {
            TransactionActor::User(u) => u == id,
            TransactionActor::Cash => false,
        }).cloned().collect::<Vec<_>>();
        Some((u, t))
    }

    pub fn users(&self) -> Result<Vec<User>, String> {
        let data = self.0.get_data(true).map_err(|e| format!("{:?}", e))?;
        Ok(data.users.into_values().collect())
    }

    pub fn transactions(&self) -> Result<Vec<Transaction>, String> {
        let data = self.0.get_data(true).map_err(|e| format!("{:?}", e))?;
        Ok(data.transactions)
    }

    pub fn apply_cart_to_user(&self, id: &str, cart: &crate::Cart) -> Result<User, String> {
        self.0.load().map_err(|e| format!("{:?}", e))?;

        let u = {
            let mut data = self.0.borrow_data_mut().map_err(|e| format!("{:?}", e))?;
            let user = data.users.get_mut(id);

            let u = match user {
                None => return Err(format!("user {} does not exist", id)),
                Some(u) => {
                    u.balance -= cart.total() as i32;
                    u.clone()
                }
            };

            data.transactions.push(Transaction {
                timestamp: Utc::now(),
                actor: TransactionActor::User(id.to_string()),
                transaction: TransactionType::Purchase {
                    products: cart.products.clone(),
                    total: cart.total(),
                }
            });

            u
        };

        self.0.save().map_err(|e| format!("{:?}", e))?;
        Ok(u)
    }

    pub fn deposit_user(&self, id: &str, amount: u32, method: DepositMethod) -> Result<User, String> {
        self.0.load().map_err(|e| format!("{:?}", e))?;

        let u = {
            let mut data = self.0.borrow_data_mut().map_err(|e| format!("{:?}", e))?;
            let user = data.users.get_mut(id);

            let u = match user {
                None => return Err(format!("user {} does not exist", id)),
                Some(u) => {
                    u.balance += amount as i32;
                    u.clone()
                }
            };

            data.transactions.push(Transaction {
                timestamp: Utc::now(),
                actor: TransactionActor::User(id.to_string()),
                transaction: TransactionType::Deposit {
                    amount,
                    method,
                }
            });

            u
        };

        self.0.save().map_err(|e| format!("{:?}", e))?;
        Ok(u)
    }

    pub fn add_user(&self, id: &str) -> Result<(), String> {
        self.0.load().map_err(|e| format!("{:?}", e))?;

        {
            let mut data = self.0.borrow_data_mut().map_err(|e| format!("{:?}", e))?;

            if data.users.contains_key(id) {
                return Err(format!("user {} already exists", id));
            }

            data.users.insert(id.to_string(), User {
                id: id.to_string(),
                balance: 0,
            });
        }

        self.0.save().map_err(|e| format!("{:?}", e))?;
        Ok(())
    }
}