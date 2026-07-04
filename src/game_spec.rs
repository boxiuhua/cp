// 彩票开奖结构描述:每种彩票 = 若干组件。仅用于真实数据的随机性分析,
// 与第 1-2 章的头奖概率/返奖率 Game 是不同关注点,故独立。

pub(crate) enum Component {
    // 无放回抽 pick 个,值 ∈ [1,size],互异
    Pool { label: &'static str, size: u32, pick: u32 },
    // 逐位独立,第 i 位 ∈ [0,bases[i]),允许重复
    Digits { label: &'static str, bases: Vec<u32> },
}

impl Component {
    // 该组件在一行数据里占用多少个号码字段
    pub fn width(&self) -> usize {
        match self {
            Component::Pool { pick, .. } => *pick as usize,
            Component::Digits { bases, .. } => bases.len(),
        }
    }
}

pub(crate) struct GameSpec {
    pub key: &'static str,
    pub name: &'static str,
    pub file: &'static str,
    pub components: Vec<Component>,
}

impl GameSpec {
    // 一行 CSV 的期望字段数 = 期号 + 日期 + 全部号码
    pub fn field_count(&self) -> usize {
        2 + self.components.iter().map(|c| c.width()).sum::<usize>()
    }
}

pub(crate) fn real_data_games() -> Vec<GameSpec> {
    vec![
        GameSpec {
            key: "ssq", name: "双色球", file: "data/ssq.csv",
            components: vec![
                Component::Pool { label: "红球", size: 33, pick: 6 },
                Component::Pool { label: "蓝球", size: 16, pick: 1 },
            ],
        },
        GameSpec {
            key: "dlt", name: "超级大乐透", file: "data/dlt.csv",
            components: vec![
                Component::Pool { label: "前区", size: 35, pick: 5 },
                Component::Pool { label: "后区", size: 12, pick: 2 },
            ],
        },
        GameSpec {
            key: "d3", name: "福彩3D", file: "data/d3.csv",
            components: vec![Component::Digits { label: "直选", bases: vec![10, 10, 10] }],
        },
        GameSpec {
            key: "pl3", name: "排列3", file: "data/pl3.csv",
            components: vec![Component::Digits { label: "直选", bases: vec![10, 10, 10] }],
        },
        GameSpec {
            key: "pl5", name: "排列5", file: "data/pl5.csv",
            components: vec![Component::Digits { label: "直选", bases: vec![10, 10, 10, 10, 10] }],
        },
        GameSpec {
            key: "qxc", name: "7星彩", file: "data/qxc.csv",
            components: vec![Component::Digits { label: "直选", bases: vec![10, 10, 10, 10, 10, 10, 15] }],
        },
        GameSpec {
            key: "qlc", name: "7乐彩", file: "data/qlc.csv",
            components: vec![Component::Pool { label: "号码", size: 30, pick: 7 }],
        },
        GameSpec {
            key: "kl8", name: "快乐8", file: "data/kl8.csv",
            components: vec![Component::Pool { label: "号码", size: 80, pick: 20 }],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_counts_are_correct() {
        let games = real_data_games();
        let by = |k: &str| games.iter().find(|g| g.key == k).unwrap().field_count();
        assert_eq!(by("ssq"), 9); // 期号+日期+6红+1蓝
        assert_eq!(by("dlt"), 9); // +5+2
        assert_eq!(by("d3"), 5);  // +3
        assert_eq!(by("pl5"), 7); // +5
        assert_eq!(by("qxc"), 9); // +7
        assert_eq!(by("kl8"), 22); // +20
    }

    #[test]
    fn all_eight_games_present() {
        assert_eq!(real_data_games().len(), 8);
    }
}
