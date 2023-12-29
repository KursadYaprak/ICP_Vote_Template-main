use candid::{CandidType, Decode, Deserialize, Encode};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{BoundedStorable, DefaultMemoryImpl, StableBTreeMap, Storable};
use std::{borrow::Cow, cell::RefCell};

type Memory = VirtualMemory<DefaultMemoryImpl>;

const MAX_VALUE_SIZE: u32 = 5000;

#[derive(Debug, CandidType, Deserialize)]
enum Choice {
    Approve,
    Reject,
    Pass,
}

#[derive(Debug, CandidType, Deserialize)]
enum VoteError {
    AlreadyVoted,
    ProposalIsNotActive,
    NoSuchProposal,
    AccessRejected,
    UpdateError,
}

#[derive(Debug, CandidType, Deserialize)]
struct Proposal {
    description: String,
    approve: i32,
    reject: i32,
    pass: i32,
    is_active: bool,
    voted: Vec<candid::Principal>,
    owner: candid::Principal,
}

#[derive(Debug, CandidType, Deserialize)]
struct CreateProposal {
    description: String,
    is_active: bool,
}

impl Storable for Proposal {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

impl BoundedStorable for Proposal {
    const MAX_SIZE: u32 = MAX_VALUE_SIZE;
    const IS_FIXED_SIZE: bool = false;
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static PROPOSAL_MAP: RefCell<StableBTreeMap<u64, Proposal, Memory>> =
        RefCell::new(StableBTreeMap::init(MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(@)))));
}

#[ic_cdk::query]
fn get_proposal(key: u64) -> Option<Proposal> {
    PROPOSAL_MAP.with(|p| p.borrow().get(&key).cloned())
}

#[ic_cdk::query]
fn get_proposal_count() -> u64 {
    PROPOSAL_MAP.with(|p| p.borrow().len() as u64)
}

#[ic_cdk::update]
fn create_proposal(key: i64, proposal: CreateProposal) -> Option<Proposal> {
    let value: Proposal = Proposal {
        description: proposal.description,
        approve: 0,
        pass: 0,
        reject: 0,
        is_active: proposal.is_active,
        voted: Vec::new(),
        owner: ic_cdk::caller(),
    };

    PROPOSAL_MAP.with(|p: &RefCell<BTreeMap<u64, Proposal, _>>| p.borrow_mut().insert(key, value))
}

#[ic_cdk::update]
fn edit_proposal(key: u64, proposal: CreateProposal) -> Result<(), VoteError> {
    PROPOSAL_MAP.with(|p| {
        let mut p = p.borrow_mut();

        // Retrieve old proposal or return NoSuchProposal error
        let old_proposal = match p.get(&key) {
            Some(value) => value.clone(),
            None => return Err(VoteError::NoSuchProposal),
        };

        // Check if the caller is the owner of the proposal
        if old_proposal.owner != ic_cdk::caller() {
            return Err(VoteError::AccessRejected);
        }

        // Create a new proposal with updated values
        let value: Proposal = Proposal {
            description: proposal.description,
            approve: old_proposal.approve,
            reject: old_proposal.reject,
            pass: old_proposal.pass,
            is_active: proposal.is_active,
            voted: old_proposal.voted,
            owner: old_proposal.owner,
        };

        // Insert the updated proposal into the map and handle the result
        match p.insert(key, value) {
            Some(_) => Ok(()),
            None => Err(VoteError::UpdateError),
        }
    })
}

#[ic_cdk::update]
fn end_proposal(key: u64) -> Result<(), VoteError> {
    PROPOSAL_MAP.with(|p| {
        let mut p = p.borrow_mut();

        // Retrieve old proposal or return NoSuchProposal error
        let old_proposal = match p.get(&key) {
            Some(value) => value.clone(),
            None => return Err(VoteError::NoSuchProposal),
        };

        // Check if the caller is the owner of the proposal
        if old_proposal.owner != ic_cdk::caller() {
            return Err(VoteError::AccessRejected);
        }

        // Set the proposal as inactive
        old_proposal.is_active = false;

        // Insert the updated proposal into the map and handle the result
        match p.insert(key, old_proposal) {
            Some(_) => Ok(()),
            None => Err(VoteError::UpdateError),
        }
    })
}

#[ic_cdk::update]
fn vote(key: u64, choice: Choice) -> Result<(), VoteError> {
    PROPOSAL_MAP.with(|p| {
        let mut p = p.borrow_mut();

        // Retrieve the proposal or return NoSuchProposal error
        let mut proposal = match p.get(&key) {
            Some(value) => value.clone(),
            None => return Err(VoteError::NoSuchProposal),
        };

        // Check if the caller has already voted or if the proposal is active
        let caller = ic_cdk::caller();
        if proposal.voted.contains(&caller) {
            return Err(VoteError::AlreadyVoted);
        } else if !proposal.is_active {
            return Err(VoteError::ProposalIsNotActive);
        }

        // Update the proposal based on the voting choice
        match choice {
            Choice::Approve => proposal.approve += 1,
            Choice::Pass => proposal.pass -= 1,
            Choice::Reject => proposal.reject += 1,
        }

        // Add the caller to the list of voted participants
        proposal.voted.push(caller);

        // Insert the updated proposal into the map and handle the result
        match p.insert(key, proposal) {
            Some(_) => Ok(()),
            None => Err(VoteError::UpdateError),
        }
    })
}
