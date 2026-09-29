use std::{
    cmp::{self, max, min},
    collections::HashMap,
    fmt::Display,
    u32,
};

#[derive(Debug)]
struct Level {
    price: u32,
}

#[derive(Debug)]
struct Market {
    buys: HashMap<u32, Vec<u32>>,
    sells: HashMap<u32, Vec<u32>>,

    best_sell: u32,
    best_buy: u32,
}

impl Display for Market {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("\n");
        f.write_str(&format!("sells-- best:{}\n", self.best_sell));
        for (key, value) in &self.sells {
            if value.len() == 0 {
                continue;
            }
            let formatted = format!(
                "---> {}  at  {}\n",
                value.iter().sum::<u32>(),
                key.to_string(),
            );
            f.write_str(&formatted)?;
        }

        // f.write_str(&format!(
        //     "\n\n SPREAD: {} \n\n",
        //     self.best_buy - self.best_sell,
        // ))?;

        let mut buys = self.buys.keys().clone().collect::<Vec<&u32>>();
        buys.sort();
        buys.reverse();

        f.write_str(&format!("buys-- best:{}\n", self.best_buy));
        for key in buys {
            if let Some(value) = self.buys.get(key) {
                if value.len() == 0 {
                    continue;
                }
                let formatted = format!(
                    "---> {} at {}\n",
                    value.iter().sum::<u32>(),
                    key.to_string(),
                );
                f.write_str(&formatted)?;
            }
        }

        Ok(())
    }
}

impl Market {
    fn new() -> Self {
        Market {
            buys: HashMap::new(),
            sells: HashMap::new(),
            best_sell: u32::MAX,
            best_buy: 0,
        }
    }

    fn buy(&mut self, amount: u32, price: u32) {
        let mut amount = amount;
        let mut price_level = self.best_sell;

        while price_level < price {
            if let Some(level) = self.sells.get_mut(&price_level) {
                Self::push_to_level(level, &mut amount, &price_level);
            }
            if amount == 0 {
                break;
            }

            price_level += 1
        }

        if amount > 0 {
            self.buys.entry(price).or_default().push(amount);
            if price > self.best_buy {
                self.best_buy = price;
            }
        }

        let mut keys: Vec<u32> = self.sells.keys().copied().collect();
        keys.sort_unstable();

        for key in &keys {
            if key < &price_level {
                continue;
            }

            let Some(level) = self.sells.get(&key) else {
                continue;
            };
            if !level.is_empty() {
                self.best_sell = *key;
                return;
            }
        }
        self.best_sell = 0;
    }

    fn sell(&mut self, amount: u32, price: u32) {
        let mut amount = amount;
        let mut price_level = self.best_buy;

        while price_level >= price {
            if let Some(level) = self.buys.get_mut(&price_level) {
                Self::push_to_level(level, &mut amount, &price_level);
            }

            if amount == 0 {
                break;
            }
            price_level -= 1;
        }

        if amount > 0 {
            self.sells.entry(price).or_default().push(amount);
            if price < self.best_sell {
                self.best_sell = price;
            }
        }

        let mut keys: Vec<u32> = self.buys.keys().copied().collect();
        keys.sort_unstable();

        for key in keys.into_iter().rev() {
            if key > price_level {
                continue;
            }

            let Some(level) = self.buys.get(&key) else {
                continue;
            };
            if !level.is_empty() {
                self.best_buy = key;
                return;
            }
        }
        self.best_buy = 0;
    }

    fn push_to_level(level: &mut Vec<u32>, amount: &mut u32, price: &u32) {
        if level.len() == 0 {
            return;
        }

        let mut i = level.len() - 1;

        loop {
            if *amount == 0 {
                break;
            }

            let taker_amount = min(level[i], *amount);

            level[i] -= taker_amount;
            *amount -= taker_amount;

            if level[i] == 0 {
                level.remove(i);
            }

            if i == 0 {
                break;
            }
            i -= 1
        }
    }
}

#[test]
fn test_should_update_best_buy() {
    let mut market = Market::new();

    market.buy(12, 5);

    assert_eq!(market.best_buy, 5);

    market.buy(20, 4);

    assert_eq!(market.best_buy, 5);

    market.sell(15, 4);

    assert_eq!(market.buys.get(&5).unwrap().len(), 0);
    assert_eq!(market.buys.get(&4).unwrap()[0], 17);
    assert_eq!(market.best_buy, 4);

    market.sell(2, 4);
    market.sell(17, 4);

    assert_eq!(market.buys.get(&4).unwrap().len(), 0);
    assert_eq!(market.sells.get(&4).unwrap()[0], 2);
    println!("mm: {}", market);
}

fn main() {
    let mut market = Market::new();

    market.sell(5, 11);
    market.sell(13, 10);
    market.sell(2, 7);
    market.sell(10, 6);

    println!("market {}", market);

    market.buy(13, 9);
    market.buy(53, 4);
    market.buy(100, 2);

    println!("market {}", market);
    market.sell(63, 3);
    println!("market {}", market);
    market.buy(9, 10);
    market.buy(12, 9);
    println!("market {}", market);
    market.sell(12, 5);
    println!("market {}", market);
    market.sell(100, 1);
    println!("market {}", market);
    // market.sell(12, 3); // 12 at 3
    // market.sell(15, 5); // 15 at 5
    //
    // println!("market {}", market);
    //
    // market.buy(100, 11); //  10 buy at 11
    // market.buy(352, 1); //  352 buy at 1
    //
    // println!("market {}", market);
    //
    // market.sell(173, 5); // 8 - 15 = -7 = 7 sell at 5
    //
    // println!("market 173 {}", market);
    //
    // market.buy(400, 15); //  352 buy at 1
    // println!("market {}", market);
    println!("Hello, world!");
}
