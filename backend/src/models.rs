#[derive(Clone, Debug)]
pub struct Map {
    pub name: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub actions: Vec<(Pixel, Action)>,
}

#[derive(Clone, Copy, Debug)]
pub struct Pixel(pub i32, pub i32);

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Move,
    Wait,
    Attack,
}

#[derive(Clone, Debug)]
pub struct Character {
    pub name: String,
    pub keys: Vec<(String, SkillKey)>,
}

#[derive(Clone, Copy, Debug)]
pub enum SkillKey {
    ErdaShower(char),
    SolJanus(char),
    RopeLift(char),
    Attack(char),
}
