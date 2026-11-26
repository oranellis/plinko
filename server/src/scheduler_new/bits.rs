// modules {{{
use crate::scheduler_new::day::Day;
// }}}

/// A set of days as bits, one word for every 64 days
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bits {
    words: Vec<u64>,
    len: usize,
}

// impl Bits {{{
impl Bits {
    pub fn new(len: usize) -> Self {
        Self {
            words: vec![0; len.div_ceil(64)],
            len,
        }
    }

    pub fn from_fn(len: usize, set: impl Fn(usize) -> bool) -> Self {
        let mut bits = Self::new(len);
        for i in (0..len).filter(|i| set(*i)) {
            bits.set(Day(i as u32), true);
        }
        bits
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, day: Day) -> bool {
        let i = day.index();
        i < self.len && self.words[i / 64] >> (i % 64) & 1 == 1
    }

    pub fn set(&mut self, day: Day, on: bool) {
        let i = day.index();
        if i < self.len {
            let bit = 1u64 << (i % 64);
            match on {
                true => self.words[i / 64] |= bit,
                false => self.words[i / 64] &= !bit,
            }
        }
    }

    /// The days in both
    pub fn and(&self, other: &Bits) -> Bits {
        let words = self.words.iter().zip(&other.words);
        Bits {
            words: words.map(|(a, b)| a & b).collect(),
            len: self.len.min(other.len),
        }
    }

    /// The days in either
    pub fn or(&self, other: &Bits) -> Bits {
        let words = self.words.iter().zip(&other.words);
        Bits {
            words: words.map(|(a, b)| a | b).collect(),
            len: self.len.min(other.len),
        }
    }

    /// The word at `i` of the days in both that fall in `from..to`
    fn masked(&self, other: &Bits, i: usize, from: usize, to: usize) -> u64 {
        let mut word = self.words[i] & other.words[i];
        let base = i * 64;
        if from > base {
            word &= u64::MAX << (from - base);
        }
        if to < base + 64 {
            word &= !(u64::MAX << (to - base));
        }
        word
    }

    /// The first day in `from..to` that is in both
    pub fn first_common(&self, other: &Bits, from: Day, to: Day) -> Option<Day> {
        let (from, to) = (from.index(), to.index().min(self.len).min(other.len));
        (from < to).then_some(())?;
        (from / 64..=(to - 1) / 64).find_map(|i| {
            let word = self.masked(other, i, from, to);
            (word != 0).then(|| Day((i * 64) as u32 + word.trailing_zeros()))
        })
    }

    /// The last day in `from..to` that is in both
    pub fn last_common(&self, other: &Bits, from: Day, to: Day) -> Option<Day> {
        let (from, to) = (from.index(), to.index().min(self.len).min(other.len));
        (from < to).then_some(())?;
        (from / 64..=(to - 1) / 64).rev().find_map(|i| {
            let word = self.masked(other, i, from, to);
            (word != 0).then(|| Day((i * 64) as u32 + 63 - word.leading_zeros()))
        })
    }
}
// }}}

/// A set of days that answers "the n-th day from here" by counting, not walking
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Valid {
    bits: Bits,
    /// How many days are set before each word
    before: Vec<u32>,
}

// impl Valid {{{
impl Valid {
    pub fn new(bits: Bits) -> Self {
        let mut total = 0;
        let before = bits
            .words
            .iter()
            .map(|w| {
                let before = total;
                total += w.count_ones();
                before
            })
            .collect();
        Self { bits, before }
    }

    pub fn bits(&self) -> &Bits {
        &self.bits
    }

    pub fn has(&self, day: Day) -> bool {
        self.bits.get(day)
    }

    /// How many days are set at all
    pub fn count(&self) -> u32 {
        self.rank(Day(self.bits.len as u32))
    }

    /// How many set days come before `day`
    pub fn rank(&self, day: Day) -> u32 {
        let i = day.index().min(self.bits.len);
        if i / 64 >= self.bits.words.len() {
            return self.before.last().map_or(0, |b| {
                b + self.bits.words.last().map_or(0, |w| w.count_ones())
            });
        }
        let within = self.bits.words[i / 64] & !(u64::MAX << (i % 64));
        self.before[i / 64] + within.count_ones()
    }

    /// The set day with `k` set days before it
    pub fn select(&self, k: u32) -> Option<Day> {
        let word = self.before.partition_point(|b| *b <= k).checked_sub(1)?;
        let mut bits = self.bits.words[word];
        let mut left = k - self.before[word];
        (left < bits.count_ones()).then_some(())?;
        while left > 0 {
            bits &= bits - 1;
            left -= 1;
        }
        Some(Day((word * 64) as u32 + bits.trailing_zeros()))
    }

    /// The first set day on or after `from`
    pub fn next(&self, from: Day) -> Option<Day> {
        self.select(self.rank(from))
    }

    /// The last set day before `to`
    pub fn previous(&self, to: Day) -> Option<Day> {
        self.select(self.rank(to).checked_sub(1)?)
    }

    /// The `n`-th set day counting from `from`, itself included, `n` at least 1
    pub fn nth_from(&self, from: Day, n: u32) -> Option<Day> {
        self.select(self.rank(from) + n.checked_sub(1)?)
    }

    /// The `n`-th set day counting back from the last before `to`, `n` at least 1
    pub fn nth_back(&self, to: Day, n: u32) -> Option<Day> {
        self.select(self.rank(to).checked_sub(n)?)
    }

    /// The set days in `from..to`
    pub fn days(&self, from: Day, to: Day) -> impl Iterator<Item = Day> + '_ {
        (self.rank(from)..self.rank(to)).filter_map(|k| self.select(k))
    }
}
// }}}

// tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    /// Weekdays of weeks that start on day 0, over 200 days
    fn weekdays() -> Valid {
        Valid::new(Bits::from_fn(200, |i| i % 7 < 5))
    }

    #[test]
    fn counting_matches_walking() {
        let valid = weekdays();
        assert_eq!(valid.count(), 144);
        assert_eq!(valid.rank(Day(7)), 5);
        assert_eq!(valid.rank(Day(500)), 144);
        assert_eq!(valid.select(5), Some(Day(7)));
        assert_eq!(valid.select(144), None);
        // across a word boundary
        let walked: Vec<Day> = (60..75).map(Day).filter(|d| valid.has(*d)).collect();
        assert_eq!(valid.days(Day(60), Day(75)).collect::<Vec<_>>(), walked);
    }

    #[test]
    fn nth_days_skip_what_is_not_set() {
        let valid = weekdays();
        // from a Saturday the first is the Monday
        assert_eq!(valid.next(Day(5)), Some(Day(7)));
        assert_eq!(valid.previous(Day(7)), Some(Day(4)));
        // Wednesday on: Wed, Thu, Fri, Mon, Tue
        assert_eq!(valid.nth_from(Day(2), 5), Some(Day(8)));
        assert_eq!(valid.nth_back(Day(9), 5), Some(Day(2)));
        assert_eq!(valid.nth_from(Day(190), 20), None);
        assert_eq!(valid.nth_back(Day(2), 5), None);
    }

    #[test]
    fn common_days_are_found_from_either_end_of_a_range() {
        let valid = weekdays();
        let mut busy = Bits::new(200);
        for day in [3, 6, 70, 130] {
            busy.set(Day(day), true);
        }
        let bits = valid.bits();
        // day 6 is a Sunday, so it is not in both
        assert_eq!(bits.first_common(&busy, Day(0), Day(200)), Some(Day(3)));
        assert_eq!(bits.first_common(&busy, Day(4), Day(200)), Some(Day(70)));
        assert_eq!(bits.last_common(&busy, Day(0), Day(200)), Some(Day(130)));
        assert_eq!(bits.last_common(&busy, Day(0), Day(130)), Some(Day(70)));
        assert_eq!(bits.first_common(&busy, Day(71), Day(130)), None);
        assert_eq!(bits.first_common(&busy, Day(9), Day(9)), None);
    }
}
// }}}
