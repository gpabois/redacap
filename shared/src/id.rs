use anyhow::bail;
use rand_xoshiro::{Xoshiro256PlusPlus, rand_core::{Rng as _, SeedableRng as _}};
use std::{str::FromStr, sync::{Arc, Mutex}};

#[derive(Clone)]
pub struct IdGenerator {
    // Utilisation de RefCell pour une mutation intérieure sûre en single-thread (WASM/Client)
    rng: Arc<Mutex<Xoshiro256PlusPlus>>,
    session_prefix: u64,
}

#[derive(Hash, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ID(u64, u64);

impl std::fmt::Display for ID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("{:016x}{:016x}", self.0, self.1))
    }
}

impl FromStr for ID {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 32 { bail!("ID parsing error : expecting a 32 long string") }
        let id1 = u64::from_str_radix(&s[0..16], 16)?;
        let id2 = u64::from_str_radix(&s[16..32], 16)?;
        Ok(Self(id1, id2))
    }
}

impl IdGenerator {
    /// Initialise le générateur avec une seed unique par session (via getrandom)
    pub fn new() -> Self {
        // 1. On génère une seed aléatoire sécurisée pour cette session
        let mut seed = [0u8; 32];
        if let Err(_) = getrandom::getrandom(&mut seed) {
            // Fallback dégradé au cas où, mais js/getrandom gère ça sur le web
            seed = [42; 32]; 
        }

        // 2. On instancie le RNG avec cette seed
        let mut master_rng = Xoshiro256PlusPlus::from_seed(seed);
        
        // 3. On extrait un préfixe unique pour cette session
        let session_prefix = master_rng.next_u64();

        Self {
            rng: Arc::new(Mutex::new(master_rng)),
            session_prefix,
        }
    }

    /// Génère un identifiant unique sous forme de chaîne (ex: "id-session-séquence")
    pub fn next_id_str(&self) -> String {
        let mut rng = self.rng.lock().unwrap();
        let local_id = rng.next_u64();
        // Le format combine le préfixe de session et un nombre aléatoire local
        format!("{:x}{:x}", self.session_prefix, local_id)
    }

    pub fn next_id(&self) -> ID {
        let mut rng = self.rng.lock().unwrap();
        let local_id = rng.next_u64();
        ID(self.session_prefix, local_id)
    }
}
