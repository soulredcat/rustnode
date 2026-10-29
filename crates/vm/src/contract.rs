//! Contract code and state as the world state stores them
//! (`StatefulContract.HalfDecoded`, `ContractStorageState`,
//! `ContractImmutableState`), and loaded contract objects.

use std::cell::RefCell;
use std::rc::Rc;

use alephium_model::instr::Context;
use alephium_model::script::Method;
use alephium_model::{Codec, Hash, OutputRef, Reader, Val, deserialize, serialize};

use crate::{Fail, VmResult, fail};

/// `Either` tags of `ContractStorageImmutableState`.
pub const IMMUTABLE_STATE_TAG: u8 = 0;
pub const CODE_TAG: u8 = 1;

/// `StatefulContract.HalfDecoded`: methods are decoded when first called.
#[derive(Debug)]
pub struct Code {
    pub hash: Hash,
    pub field_length: i32,
    method_ends: Vec<i32>,
    methods_bytes: Vec<u8>,
    decoded: RefCell<Vec<Option<Rc<Method>>>>,
    /// `searchedMethodIndex` and `cachedSelectors` of the selector search.
    search: RefCell<(i64, std::collections::HashMap<i32, usize>)>,
}

impl Code {
    /// The serialized form: compact field length, method end offsets, then
    /// the methods' bytes; its Blake2b hash is the code hash.
    pub fn decode(bytes: &[u8]) -> VmResult<Self> {
        let mut reader = Reader::new(bytes);
        let field_length = reader.int().map_err(|error| Fail(error.to_string()))?;
        let method_ends =
            Vec::<i32>::decode(&mut reader).map_err(|error| Fail(error.to_string()))?;
        let length = usize::try_from(method_ends.last().copied().unwrap_or(0))
            .map_err(|_| Fail("negative method offset".to_owned()))?;
        let methods_bytes = reader
            .take(length)
            .map_err(|error| Fail(error.to_string()))?
            .to_vec();
        if reader.remaining() != 0 {
            return fail("trailing bytes after contract code");
        }
        let methods = method_ends.len();
        Ok(Self {
            hash: Hash::blake2b(bytes),
            field_length,
            method_ends,
            methods_bytes,
            decoded: RefCell::new(vec![None; methods]),
            search: RefCell::new((-1, std::collections::HashMap::new())),
        })
    }

    /// `StatefulContract.toHalfDecoded`.
    pub fn from_methods(field_length: i32, methods: &[Method]) -> VmResult<Self> {
        let mut methods_bytes = Vec::new();
        let mut method_ends = Vec::with_capacity(methods.len());
        for method in methods {
            method.encode(&mut methods_bytes);
            method_ends.push(
                i32::try_from(methods_bytes.len())
                    .map_err(|_| Fail("code too large".to_owned()))?,
            );
        }
        let mut bytes = Vec::new();
        field_length.encode(&mut bytes);
        method_ends.encode(&mut bytes);
        bytes.extend_from_slice(&methods_bytes);
        Self::decode(&bytes)
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.field_length.encode(&mut bytes);
        self.method_ends.encode(&mut bytes);
        bytes.extend_from_slice(&self.methods_bytes);
        bytes
    }

    #[must_use]
    pub fn methods_length(&self) -> usize {
        self.method_ends.len()
    }

    fn method_bytes(&self, index: usize) -> VmResult<&[u8]> {
        let start = if index == 0 {
            0
        } else {
            usize::try_from(self.method_ends[index - 1])
                .map_err(|_| Fail("bad offset".to_owned()))?
        };
        let end =
            usize::try_from(self.method_ends[index]).map_err(|_| Fail("bad offset".to_owned()))?;
        self.methods_bytes
            .get(start..end)
            .ok_or_else(|| Fail("method offsets out of range".to_owned()))
    }

    pub fn method(&self, index: usize) -> VmResult<Rc<Method>> {
        if index >= self.methods_length() {
            return fail(format!("invalid method index {index}"));
        }
        if let Some(method) = &self.decoded.borrow()[index] {
            return Ok(Rc::clone(method));
        }
        let bytes = self.method_bytes(index)?;
        let mut reader = Reader::new(bytes);
        let method = Method::decode(&mut reader, Context::Stateful)
            .map_err(|error| Fail(format!("method {index}: {error}")))?;
        if reader.remaining() != 0 {
            return fail("trailing bytes after a method");
        }
        let method = Rc::new(method);
        self.decoded.borrow_mut()[index] = Some(Rc::clone(&method));
        Ok(method)
    }

    /// The selector of a public method starting with `MethodSelector`.
    fn selector(&self, index: usize) -> VmResult<Option<i32>> {
        let method = self.method(index)?;
        Ok(match method.instrs.first() {
            Some(first) if method.is_public && first.name() == "MethodSelector" => {
                match first.operand {
                    alephium_model::instr::Operand::Selector(selector) => Some(selector),
                    _ => None,
                }
            }
            _ => None,
        })
    }

    /// `getMethodBySelector`: the method index and how many methods the
    /// search went through (`methodSearched`, charged as gas); found
    /// selectors are remembered for the code object's lifetime.
    pub fn method_by_selector(&self, selector: i32) -> VmResult<(usize, u64)> {
        if let Some(index) = self.search.borrow().1.get(&selector) {
            return Ok((*index, 0));
        }
        let searched = self.search.borrow().0;
        let mut index = usize::try_from(searched + 1).unwrap_or(0);
        let mut found = false;
        while !found && index < self.methods_length() {
            match self.selector(index)? {
                Some(at) => {
                    self.search.borrow_mut().1.insert(at, index);
                    if at == selector {
                        found = true;
                    } else {
                        index += 1;
                    }
                }
                None => index += 1,
            }
        }
        let index_i64 = i64::try_from(index).unwrap_or(i64::MAX);
        self.search.borrow_mut().0 = index_i64;
        if found {
            Ok((index, u64::try_from(index_i64 - searched).unwrap_or(0)))
        } else {
            fail(format!("invalid method selector {selector}"))
        }
    }

    /// The methods' bytes, for `estimateContractLoadByteSize`.
    #[must_use]
    pub fn methods_bytes_len(&self) -> usize {
        self.methods_bytes.len()
    }

    /// `Contract.initialStateHash`.
    #[must_use]
    pub fn initial_state_hash(&self, imm: &[Val], mutable: &[Val]) -> Hash {
        let mut bytes = self.hash.0.to_vec();
        let fields: Vec<Val> = imm.iter().chain(mutable).cloned().collect();
        fields.encode(&mut bytes);
        Hash::blake2b(&Hash::blake2b(&bytes).0)
    }
}

/// `ContractStorageState` as stored in the contract trie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredState {
    /// `ContractMutableState`, since Leman.
    Mutable {
        mut_fields: Vec<Val>,
        output_ref: OutputRef,
        immutable_hash: Hash,
    },
    /// `ContractLegacyState`, before Leman.
    Legacy {
        code_hash: Hash,
        initial_state_hash: Hash,
        mut_fields: Vec<Val>,
        output_ref: OutputRef,
    },
}

impl StoredState {
    pub fn decode(bytes: &[u8]) -> VmResult<Self> {
        if let Ok((mut_fields, (output_ref, immutable_hash))) =
            deserialize::<(Vec<Val>, (OutputRef, Hash))>(bytes)
        {
            return Ok(Self::Mutable {
                mut_fields,
                output_ref,
                immutable_hash,
            });
        }
        let (code_hash, (initial_state_hash, (mut_fields, output_ref))) =
            deserialize::<(Hash, (Hash, (Vec<Val>, OutputRef)))>(bytes)
                .map_err(|error| Fail(format!("contract state: {error}")))?;
        Ok(Self::Legacy {
            code_hash,
            initial_state_hash,
            mut_fields,
            output_ref,
        })
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Mutable {
                mut_fields,
                output_ref,
                immutable_hash,
            } => serialize(&(mut_fields.clone(), (*output_ref, *immutable_hash))),
            Self::Legacy {
                code_hash,
                initial_state_hash,
                mut_fields,
                output_ref,
            } => serialize(&(
                *code_hash,
                (*initial_state_hash, (mut_fields.clone(), *output_ref)),
            )),
        }
    }

    #[must_use]
    pub const fn output_ref(&self) -> &OutputRef {
        match self {
            Self::Mutable { output_ref, .. } | Self::Legacy { output_ref, .. } => output_ref,
        }
    }

    pub fn set_output_ref(&mut self, new: OutputRef) {
        match self {
            Self::Mutable { output_ref, .. } | Self::Legacy { output_ref, .. } => *output_ref = new,
        }
    }

    pub fn set_mut_fields(&mut self, new: Vec<Val>) {
        match self {
            Self::Mutable { mut_fields, .. } | Self::Legacy { mut_fields, .. } => *mut_fields = new,
        }
    }
}

/// `ContractImmutableState`: code hash, initial state hash, immutable fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmutableState {
    pub code_hash: Hash,
    pub initial_state_hash: Hash,
    pub imm_fields: Vec<Val>,
}

impl ImmutableState {
    pub fn decode(bytes: &[u8]) -> VmResult<Self> {
        let (code_hash, (initial_state_hash, imm_fields)) =
            deserialize::<(Hash, (Hash, Vec<Val>))>(bytes)
                .map_err(|error| Fail(format!("immutable state: {error}")))?;
        Ok(Self {
            code_hash,
            initial_state_hash,
            imm_fields,
        })
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        serialize(&(
            self.code_hash,
            (self.initial_state_hash, self.imm_fields.clone()),
        ))
    }

    /// `stateHash`, the key it is stored under.
    #[must_use]
    pub fn hash(&self) -> Hash {
        Hash::blake2b(&self.encode())
    }
}

/// `StatefulContractObject`: a loaded contract.
#[derive(Debug)]
pub struct ContractObj {
    pub id: Hash,
    pub code: Rc<Code>,
    pub initial_state_hash: Hash,
    pub imm_fields: Vec<Val>,
    pub mut_fields: Vec<Val>,
    pub initial_mut_fields: Vec<Val>,
}

impl ContractObj {
    /// `isUpdated`.
    #[must_use]
    pub fn is_updated(&self) -> bool {
        self.mut_fields != self.initial_mut_fields
    }
}
