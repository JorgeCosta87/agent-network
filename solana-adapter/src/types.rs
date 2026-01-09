#[derive(Clone, Debug)]
pub struct AccountFilter {
    pub offset: usize,
    pub value: Vec<u8>,
}
