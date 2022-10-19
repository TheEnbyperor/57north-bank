#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Clone)]
pub enum Barcode {
    Ean13(Vec<u8>),
    Ean8(Vec<u8>),
    UpcA(Vec<u8>),
    UpcE(Vec<u8>),
}

impl std::fmt::Display for Barcode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Barcode::Ean13(digits) => {
                write!(f, "EAN-13: {}", digits.iter().map(|d| d.to_string()).collect::<String>())
            }
            Barcode::Ean8(digits) => {
                write!(f, "EAN-8: {}", digits.iter().map(|d| d.to_string()).collect::<String>())
            }
            Barcode::UpcA(digits) => {
                write!(f, "UPC-A: {}", digits.iter().map(|d| d.to_string()).collect::<String>())
            }
            Barcode::UpcE(digits) => {
                write!(f, "UPC-E: {}", digits.iter().map(|d| d.to_string()).collect::<String>())
            }
        }
    }
}

impl Barcode {
    pub fn try_parse(input: &str) -> Option<Self> {
        let barcode_digits = int_digits(input)?;
        match barcode_digits.len() {
            13 => Some(Self::Ean13(barcode_digits)),
            12 => Some(Self::UpcA(barcode_digits)),
            8 => Some(Self::Ean8(barcode_digits)),
            6 => Some(Self::UpcE(barcode_digits)),
            _ => None
        }
    }

    pub fn check_digit(&self) -> bool {
        let check = |digits: &[u8], invert: bool| -> bool {
            let (odd, even): (Vec<_>, Vec<_>) = digits[..]
                .iter().enumerate()
                .partition(|&x| if invert {
                    x.0 % 2 == 0
                } else {
                    x.0 % 2 == 1
                });
            let sum = even.iter().map(|x| *x.1 as u32).sum::<u32>() +
                (odd.iter().map(|x| *x.1 as u32).sum::<u32>() * 3);
            sum % 10 == 0
        };

        match self {
            Barcode::Ean13(digits) => {
                if digits.len() != 13 {
                    return false;
                }

                check(digits, false)
            }
            Barcode::Ean8(digits) => {
                if digits.len() != 8 {
                    return false;
                }

                check(digits, true)
            }
            Barcode::UpcA(digits) => {
                if digits.len() != 12 {
                    return false;
                }

                check(digits, false)
            }
            Barcode::UpcE(digits) => {
                if digits.len() != 6 {
                    return false;
                }

                check(digits, false)
            }
        }
    }
}

fn int_digits(input: &str) -> Option<Vec<u8>> {
    input.chars().map(|d| Some(d.to_digit(10)? as u8)).collect::<Option<Vec<_>>>()
}