use std::str::FromStr;

#[derive(Debug, Clone, Copy)]
pub struct PrNumber(pub u64);

impl FromStr for PrNumber {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let number = value
            .parse::<u64>()
            .map_err(|_| "expected a positive pull request number")?;
        if number == 0 {
            return Err("expected a positive pull request number");
        }
        Ok(Self(number))
    }
}
