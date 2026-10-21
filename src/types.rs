// NOTE: these serde structs are not completely fleshed out just added bare min so take them with a
// grain of salt

use serde::Deserialize;

/// https://github.com/firefox-devtools/profiler/blob/main/docs-developer/gecko-profile-format.md
/// From https://firefox-source-docs.mozilla.org/tools/profiler/code-overview.html:
/// For example, if we have collected the following samples:
///
/// A -> B -> C
/// A -> B
/// A -> B -> D
/// The frame table would contain each frame name, something like: ["A", "B", "C", "D"]. So the frame containing “A” has index 0, “B” is at 1, etc.
///
/// The stack table would contain each stack level, something like: [[0, null], [1, 0], [2, 1], [3, 1]]. [0, null] means the frame is 0 (“A”), and it has no caller, it’s the root frame. [1, 0] means the frame is 1 (“B”), and its caller is stack 0, which is just the previous one in this example.
///
#[derive(Deserialize, Debug)]
pub struct Profile {
    pub libs: Vec<Lib>,
    pub threads: Vec<Thread>,
    pub shared: Option<Shared>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Shared {
    pub string_array: Option<Vec<String>>,
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
pub struct Thread {
    /// Contains the list of samples made by the profiler, along with their corresponding weights
    pub samples: SampleTable,

    /// Contains the different stacktrace permutations found within a complete profiler.
    ///
    /// For example, if we had:
    /// A -> B -> C -> D
    /// This would be encoded as frame D being called by prefix (A->B->C), where (A->B->C)
    /// would be another entry within the StackTable we reference to via the prefix usize
    ///
    /// For ref: https://firefox-source-docs.mozilla.org/tools/profiler/code-overview.html#json-output-of-samples
    pub stack_table: StackTable,

    /// Contains a list of all the possible deduplicated frames within a profile session.
    ///
    /// We have these guarantees from the docs:
    /// - Each unique frame string is written into a JSON array, index is frame identifier
    /// - Each stack level de-duplicated, identifies associated frame string, points at the calling stack level
    /// - Identifier for top of the stack is stored along with a timestamp
    pub frame_table: FrameTable,
    
    /// Table containing a list of functions the sampler hit along with their metadata
    pub func_table: FuncTable,

    /// Table containing a list of all resources (e.g. libraries) used within a profiler session
    pub resource_table: ResourceTable,
}

#[derive(Deserialize, Debug)]
struct StackTable {
    pub prefix: Vec<Option<usize>>,
    pub frame: Vec<usize>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FrameTable {
    // TODO: -1 in profile schema, but what cases would frame not have an address?
    /// The address of a given frame, -1 if no address
    pub address: Vec<i64>,

    /// The index into the func table describing the name and resource of a function
    pub func: Vec<usize>,

    // TODO: I haven't the slightest clue what inline_depth is, but it's part of the schema.
    // Hope it's not important! :(
    pub inline_depth: Vec<u32>,
}

#[derive(Deserialize, Debug)]
pub struct FuncTable {
    /// Name of a function.
    pub name: Vec<usize>,
    pub resource: Vec<i64>,
}

#[derive(Deserialize, Debug)]
pub struct SampleTable {
    /// For each sample, stack[i] is the optional index into the stack table if a stack exists
    pub stack: Vec<Option<usize>>,

    /// A sample can be weighted for a variety of reasons, e.g. one sample happening longer
    /// than the typical time quantum, so we can weight it higher
    pub weight: Option<Vec<f64>>,
}

#[derive(Deserialize, Debug)]
pub struct ResourceTable {
    pub lib: Vec<Option<usize>>,
}
