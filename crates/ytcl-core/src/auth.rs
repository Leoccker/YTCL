//! Contas do YouTube conectadas e onde os cookies delas ficam guardados.
//!
//! O cookie de sessão é o segredo mais sensível que o app manuseia. Ele nunca
//! toca um arquivo em texto: vive no cofre do SO (Secret Service no Linux,
//! Keychain no macOS, Credential Manager no Windows) através do `keyring`.
//!
//! O modelo já é multi-conta desde o início — cada conta tem um `id` próprio
//! e o cookie fica sob a chave `cookie:<id>`. Retrofitar isso depois, quando
//! já houvesse uma conta "única" gravada, seria bem mais caro.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Nome do serviço no cofre do SO. Bate com o `identifier` do Tauri.
const SERVICE: &str = "dev.ytcl.app";
const INDEX_KEY: &str = "accounts";

/// Uma conta conectada.
///
/// O `id` é um UUID gerado por nós: o cookie não carrega um identificador
/// estável e legível, e derivar um do YouTube exigiria uma chamada
/// autenticada só para isso.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    /// Rótulo editável pelo usuário. Nasce como "Conta 1".
    pub label: String,
    pub added_at: u64,
}

/// Abstração sobre o cofre de segredos, para os testes não dependerem de um
/// Secret Service rodando.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

/// Implementação real: o cofre do sistema operacional.
pub struct KeyringStore;

impl SecretStore for KeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(SERVICE, key)
            .map_err(|e| CoreError::Other(format!("cofre indisponível: {e}")))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CoreError::Other(format!("lendo do cofre: {e}"))),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        keyring::Entry::new(SERVICE, key)
            .and_then(|e| e.set_password(value))
            .map_err(|e| CoreError::Other(format!("gravando no cofre: {e}")))
    }

    fn delete(&self, key: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, key)
            .map_err(|e| CoreError::Other(format!("cofre indisponível: {e}")))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CoreError::Other(format!("apagando do cofre: {e}"))),
        }
    }
}

/// Operações sobre as contas. Sem estado próprio: o cofre é a fonte da
/// verdade, e cada método lê o índice atual.
pub struct AuthStore<'s> {
    store: &'s dyn SecretStore,
}

impl<'s> AuthStore<'s> {
    pub fn new(store: &'s dyn SecretStore) -> Self {
        Self { store }
    }

    fn cookie_key(id: &str) -> String {
        format!("cookie:{id}")
    }

    /// Todas as contas conectadas. Cofre vazio → lista vazia, nunca erro.
    pub fn accounts(&self) -> Result<Vec<Account>> {
        match self.store.get(INDEX_KEY)? {
            None => Ok(Vec::new()),
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| CoreError::Other(format!("índice de contas ilegível: {e}"))),
        }
    }

    fn save_index(&self, accounts: &[Account]) -> Result<()> {
        let json = serde_json::to_string(accounts)
            .map_err(|e| CoreError::Other(format!("serializando contas: {e}")))?;
        self.store.set(INDEX_KEY, &json)
    }

    pub fn cookie(&self, id: &str) -> Result<Option<String>> {
        self.store.get(&Self::cookie_key(id))
    }

    /// Grava uma conta nova e seu cookie.
    ///
    /// Só deve ser chamado depois de o cookie ter sido validado contra o
    /// YouTube — este método não valida nada, só persiste.
    pub fn add(&self, cookie: &str) -> Result<Account> {
        let mut accounts = self.accounts()?;
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            label: format!("Conta {}", accounts.len() + 1),
            added_at: crate::epoch_secs(),
        };
        self.store.set(&Self::cookie_key(&account.id), cookie)?;
        accounts.push(account.clone());
        self.save_index(&accounts)?;
        Ok(account)
    }

    /// Substitui o cookie de uma conta existente — usado quando a sessão
    /// expira e o usuário reconecta a mesma conta.
    pub fn update_cookie(&self, id: &str, cookie: &str) -> Result<()> {
        if !self.accounts()?.iter().any(|a| a.id == id) {
            return Err(CoreError::NotFound);
        }
        self.store.set(&Self::cookie_key(id), cookie)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let restantes: Vec<Account> =
            self.accounts()?.into_iter().filter(|a| a.id != id).collect();
        self.save_index(&restantes)?;
        self.store.delete(&Self::cookie_key(id))
    }

    pub fn rename(&self, id: &str, label: &str) -> Result<()> {
        let mut accounts = self.accounts()?;
        let acc = accounts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(CoreError::NotFound)?;
        acc.label = label.to_string();
        self.save_index(&accounts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemStore(Mutex<HashMap<String, String>>);

    impl SecretStore for MemStore {
        fn get(&self, key: &str) -> Result<Option<String>> {
            Ok(self.0.lock().get(key).cloned())
        }
        fn set(&self, key: &str, value: &str) -> Result<()> {
            self.0.lock().insert(key.into(), value.into());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<()> {
            self.0.lock().remove(key);
            Ok(())
        }
    }

    #[test]
    fn cofre_vazio_nao_e_erro() {
        let mem = MemStore::default();
        let auth = AuthStore::new(&mem);
        assert!(auth.accounts().unwrap().is_empty());
        assert!(auth.cookie("qualquer").unwrap().is_none());
    }

    #[test]
    fn adiciona_le_e_remove() {
        let mem = MemStore::default();
        let auth = AuthStore::new(&mem);

        let a = auth.add("SID=aaa; HSID=bbb").unwrap();
        let b = auth.add("SID=ccc").unwrap();

        assert_eq!(a.label, "Conta 1");
        assert_eq!(b.label, "Conta 2");
        assert_ne!(a.id, b.id);
        assert_eq!(auth.accounts().unwrap().len(), 2);
        assert_eq!(auth.cookie(&a.id).unwrap().as_deref(), Some("SID=aaa; HSID=bbb"));

        auth.remove(&a.id).unwrap();
        assert_eq!(auth.accounts().unwrap(), vec![b.clone()]);
        // O cookie da conta removida some junto.
        assert!(auth.cookie(&a.id).unwrap().is_none());
    }

    #[test]
    fn troca_de_cookie_ao_reconectar() {
        let mem = MemStore::default();
        let auth = AuthStore::new(&mem);
        let a = auth.add("SID=velho").unwrap();

        auth.update_cookie(&a.id, "SID=novo").unwrap();
        assert_eq!(auth.cookie(&a.id).unwrap().as_deref(), Some("SID=novo"));
        // Não criou conta nova.
        assert_eq!(auth.accounts().unwrap().len(), 1);
    }

    #[test]
    fn update_e_rename_em_conta_inexistente_falham() {
        let mem = MemStore::default();
        let auth = AuthStore::new(&mem);
        assert!(matches!(auth.update_cookie("nada", "x"), Err(CoreError::NotFound)));
        assert!(matches!(auth.rename("nada", "x"), Err(CoreError::NotFound)));
    }

    #[test]
    fn rename_persiste() {
        let mem = MemStore::default();
        let auth = AuthStore::new(&mem);
        let a = auth.add("SID=x").unwrap();
        auth.rename(&a.id, "Pessoal").unwrap();
        assert_eq!(auth.accounts().unwrap()[0].label, "Pessoal");
    }

    #[test]
    fn indice_corrompido_vira_erro_claro() {
        let mem = MemStore::default();
        mem.set("accounts", "{ isso não é json de lista").unwrap();
        let auth = AuthStore::new(&mem);
        assert!(matches!(auth.accounts(), Err(CoreError::Other(_))));
    }
}
