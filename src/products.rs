pub type Products = std::collections::HashMap<crate::barcode::Barcode, Product>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Product {
    pub barcode: crate::barcode::Barcode,
    pub name: String,
    pub price: u32,
}

impl Product {
    pub fn disp_price(&self) -> String {
        format!("£{:.2}", self.price as f64 / 100.0)
    }
}

pub fn read_products() -> Result<Products, String> {
    let mut products = std::collections::HashMap::new();

    let products_raw = match std::fs::read("./data/products") {
        Ok(p) => p,
        Err(e) => return Err(format!("cannot open products file {}", e)),
    };
    let products_str = match String::from_utf8(products_raw) {
        Ok(p) => p,
        Err(e) => return Err(format!("cannot parse products file {}", e)),
    };
    let product_lines = products_str
        .split("\n")
        .filter(|l| !l.trim().is_empty() && l.chars().nth(0).unwrap() != '#')
        .collect::<Vec<_>>();

    for line in product_lines {
        let mut left = line.clone();

        let mut take_part = || match left.split_once(" ") {
            Some(v) => {
                left = v.1;
                Ok(v.0.trim())
            },
            None => return Err(format!("invalid line {}", line))
        };

        let barcode = take_part()?;
        let price = take_part()?;
        let descriptor = left;

        let barcode = match crate::barcode::Barcode::try_parse(barcode) {
            Some(d) => d,
            None => return Err(format!("invalid barcode {}", barcode))
        };

        let price = match u32::from_str_radix(price, 10) {
            Ok(p) => p,
            Err(e) => return Err(format!("invalid price {}", e))
        };

        products.insert(barcode.clone(), Product {
            name: descriptor.to_string(),
            price,
            barcode,
        });
    }

    Ok(products)
}