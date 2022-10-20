#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Clone)]
pub struct Barcode([u8; 14]);

impl std::fmt::Display for Barcode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0.iter().map(|d| d.to_string()).collect::<String>())
    }
}

impl Barcode {
    pub fn try_parse(input: &str) -> Option<Self> {
        let d = int_digits(input)?;
        match d.len() {
            14 => Some(Self([d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8], d[9], d[10], d[11], d[12], d[13]])),
            13 => Some(Self([0, d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8], d[9], d[10], d[11], d[12]])),
            12 => Some(Self([0, 0, d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], d[8], d[9], d[10], d[11]])),
            8 => Some(Self([0, 0, 0, 0, 0, 0, d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]])),
            6 => Some(Self([0, 0, 0, 0, 0, 0, 0, 0, d[0], d[1], d[2], d[3], d[4], d[5]])),
            _ => None
        }
    }

    pub fn check_digit(&self) -> bool {
        let (odd, even): (Vec<_>, Vec<_>) = self.0.iter().enumerate().partition(|&x| x.0 % 2 == 0);
        let sum = even.iter().map(|x| *x.1 as u32).sum::<u32>() +
            (odd.iter().map(|x| *x.1 as u32).sum::<u32>() * 3);
        sum % 10 == 0
    }
}

fn int_digits(input: &str) -> Option<Vec<u8>> {
    input.chars().map(|d| Some(d.to_digit(10)? as u8)).collect::<Option<Vec<_>>>()
}