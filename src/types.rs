// NOTE: these serde structs are not completely fleshed out just added bare min so take them with a
// grain of salt

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Profile {
    pub libs: Vec<Lib>,
    pub threads: Vec<Thread>,
    pub shared: Option<Shared>
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Shared {
    pub string_array: Option<Vec<String>>
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Lib {
    pub path: String,
    pub debug_path: String,
    pub breakpad_id: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Thread {
    pub samples: SampleTable,
    pub stack_table: StackTable,
    pub frame_table: FrameTable,
    pub func_table: FuncTable,
    pub resource_table: ResourceTable,
}

#[derive(Deserialize, Debug)]
struct StackTable {
    pub prefix: Vec<Option<usize>>,
    pub frame: Vec<usize>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct FrameTable {
    pub address: Vec<i64>,
    pub func: Vec<usize>,

    pub inline_depth: Vec<u32>,
}

#[derive(Deserialize, Debug)]
struct FuncTable {
    pub name: Vec<usize>,
    pub resource: Vec<i64>
}

#[derive(Deserialize, Debug)]
struct SampleTable {
    pub stack: Vec<Option<usize>>,
    pub weight: Option<Vec<f64>>,
}

#[derive(Deserialize, Debug)]
struct ResourceTable {
    pub lib: Vec<Option<usize>>
}
