//! Fila de reprodução: faixa atual, histórico, próximas, shuffle e repeat.
//!
//! Sem dependência de random: o embaralhamento usa um `splitmix64` inline com
//! semente explícita, então um mesmo `(fila, semente)` produz sempre a mesma
//! ordem — o que torna os testes determinísticos e permite, no futuro,
//! reproduzir uma sessão.

use ytcl_core::model::Track;

use crate::state::RepeatMode;

pub struct Queue {
    /// Lista canônica, na ordem em que foi enfileirada.
    tracks: Vec<Track>,
    /// Permutação de índices de `tracks` — a ordem de tocar. Sem shuffle é
    /// `0..n`.
    order: Vec<usize>,
    /// Posição atual dentro de `order`.
    pos: usize,
    repeat: RepeatMode,
    shuffled: bool,
    seed: u64,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            order: Vec::new(),
            pos: 0,
            repeat: RepeatMode::Off,
            shuffled: false,
            seed: 0x9E3779B97F4A7C15,
        }
    }
}

/// PRNG minúsculo e determinístico. Não é criptográfico — só precisa
/// distribuir bem para um Fisher-Yates.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

impl Queue {
    /// Substitui a fila e posiciona em `start`. `start` fora do intervalo cai
    /// para 0.
    pub fn set(&mut self, tracks: Vec<Track>, start: usize) {
        let n = tracks.len();
        self.tracks = tracks;
        self.order = (0..n).collect();
        self.pos = if n == 0 { 0 } else { start.min(n - 1) };
        if self.shuffled {
            self.reshuffle_ahead();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }

    pub fn shuffled(&self) -> bool {
        self.shuffled
    }

    pub fn current(&self) -> Option<&Track> {
        self.order.get(self.pos).and_then(|&i| self.tracks.get(i))
    }

    /// A próxima faixa que tocaria, sem mexer no estado — usada para
    /// pré-resolver o stream antes de a atual acabar.
    pub fn peek_next(&self) -> Option<&Track> {
        self.next_pos().and_then(|p| self.order.get(p)).and_then(|&i| self.tracks.get(i))
    }

    /// Índice em `order` da próxima faixa, respeitando o modo de repetição.
    fn next_pos(&self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        match self.repeat {
            RepeatMode::One => Some(self.pos),
            RepeatMode::All => Some((self.pos + 1) % self.order.len()),
            RepeatMode::Off => {
                let n = self.pos + 1;
                (n < self.order.len()).then_some(n)
            }
        }
    }

    /// Avança. Devolve a nova faixa atual, ou `None` se a fila terminou
    /// (repeat Off na última).
    pub fn advance(&mut self) -> Option<&Track> {
        let p = self.next_pos()?;
        self.pos = p;
        self.current()
    }

    /// Volta uma faixa. Não sai da primeira.
    pub fn go_back(&mut self) -> Option<&Track> {
        if self.pos == 0 {
            return self.current();
        }
        self.pos -= 1;
        self.current()
    }

    /// Pula direto para um item, por índice em `order`.
    pub fn jump_to(&mut self, order_index: usize) -> Option<&Track> {
        if order_index < self.order.len() {
            self.pos = order_index;
        }
        self.current()
    }

    pub fn set_repeat(&mut self, mode: RepeatMode) {
        self.repeat = mode;
    }

    pub fn set_shuffle(&mut self, on: bool) {
        if on == self.shuffled {
            return;
        }
        self.shuffled = on;
        if on {
            self.reshuffle_ahead();
        } else {
            // Volta à ordem canônica; reposiciona no mesmo item.
            let current_track = self.order.get(self.pos).copied();
            self.order = (0..self.tracks.len()).collect();
            self.pos = current_track
                .and_then(|t| self.order.iter().position(|&i| i == t))
                .unwrap_or(0);
        }
    }

    /// Embaralha só o que está à frente da faixa atual — o histórico fica
    /// intacto. Fisher-Yates com a semente da fila.
    fn reshuffle_ahead(&mut self) {
        let start = self.pos + 1;
        if start >= self.order.len() {
            return;
        }
        let mut state = self.seed ^ (self.tracks.len() as u64).wrapping_mul(0x2545F4914F6CDD1D);
        let tail = &mut self.order[start..];
        for i in (1..tail.len()).rev() {
            let j = (splitmix64(&mut state) % (i as u64 + 1)) as usize;
            tail.swap(i, j);
        }
    }

    /// Insere logo depois da faixa atual.
    pub fn play_next(&mut self, track: Track) {
        let idx = self.tracks.len();
        self.tracks.push(track);
        let at = (self.pos + 1).min(self.order.len());
        self.order.insert(at, idx);
    }

    /// Insere no fim da fila.
    pub fn enqueue(&mut self, track: Track) {
        let idx = self.tracks.len();
        self.tracks.push(track);
        self.order.push(idx);
    }

    /// Reordena um item (drag & drop). Índices em `order`. Não move a faixa
    /// atual.
    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.order.len() || to >= self.order.len() || from == to || from == self.pos {
            return;
        }
        let item = self.order.remove(from);
        self.order.insert(to.min(self.order.len()), item);
        // Reposiciona `pos` se a remoção/inserção o deslocou.
        if from < self.pos && to >= self.pos {
            self.pos -= 1;
        } else if from > self.pos && to <= self.pos {
            self.pos += 1;
        }
    }

    /// As faixas na ordem de tocar, com um marcador para a atual — o que a
    /// UI da fila mostra.
    pub fn view(&self) -> Vec<(&Track, bool)> {
        self.order
            .iter()
            .enumerate()
            .filter_map(|(oi, &ti)| self.tracks.get(ti).map(|t| (t, oi == self.pos)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracks(ids: &[&str]) -> Vec<Track> {
        ids.iter()
            .map(|id| Track {
                id: (*id).into(),
                title: format!("t{id}"),
                artists: vec![],
                album: None,
                duration_secs: Some(180),
                art: None,
                is_explicit: false,
                set_video_id: None,
            })
            .collect()
    }

    fn ids(q: &Queue) -> Vec<String> {
        q.view().iter().map(|(t, _)| t.id.clone()).collect()
    }

    #[test]
    fn avanca_e_para_no_fim_com_repeat_off() {
        let mut q = Queue::default();
        q.set(tracks(&["a", "b", "c"]), 0);
        assert_eq!(q.current().unwrap().id, "a");
        assert_eq!(q.advance().unwrap().id, "b");
        assert_eq!(q.advance().unwrap().id, "c");
        assert!(q.advance().is_none());
    }

    #[test]
    fn repeat_all_da_a_volta() {
        let mut q = Queue::default();
        q.set(tracks(&["a", "b"]), 1);
        q.set_repeat(RepeatMode::All);
        assert_eq!(q.advance().unwrap().id, "a");
    }

    #[test]
    fn repeat_one_fica_na_mesma() {
        let mut q = Queue::default();
        q.set(tracks(&["a", "b"]), 0);
        q.set_repeat(RepeatMode::One);
        assert_eq!(q.advance().unwrap().id, "a");
        assert_eq!(q.peek_next().unwrap().id, "a");
    }

    #[test]
    fn peek_nao_altera_o_estado() {
        let mut q = Queue::default();
        q.set(tracks(&["a", "b", "c"]), 0);
        assert_eq!(q.peek_next().unwrap().id, "b");
        assert_eq!(q.current().unwrap().id, "a");
    }

    #[test]
    fn go_back_nao_passa_da_primeira() {
        let mut q = Queue::default();
        q.set(tracks(&["a", "b"]), 0);
        assert_eq!(q.go_back().unwrap().id, "a");
    }

    #[test]
    fn shuffle_e_deterministico_e_preserva_a_atual() {
        let mut a = Queue::default();
        a.set(tracks(&["1", "2", "3", "4", "5", "6"]), 1);
        a.set_shuffle(true);

        let mut b = Queue::default();
        b.set(tracks(&["1", "2", "3", "4", "5", "6"]), 1);
        b.set_shuffle(true);

        assert_eq!(ids(&a), ids(&b), "mesma fila + semente => mesma ordem");
        assert_eq!(a.current().unwrap().id, "2", "a atual não muda ao embaralhar");
        // O histórico (índice 0) fica intacto.
        assert_eq!(a.view()[0].0.id, "1");
    }

    #[test]
    fn desligar_shuffle_restaura_a_ordem() {
        let mut q = Queue::default();
        q.set(tracks(&["a", "b", "c", "d"]), 0);
        q.set_shuffle(true);
        q.set_shuffle(false);
        assert_eq!(ids(&q), vec!["a", "b", "c", "d"]);
        assert_eq!(q.current().unwrap().id, "a");
    }

    #[test]
    fn play_next_entra_logo_depois_da_atual() {
        let mut q = Queue::default();
        q.set(tracks(&["a", "b"]), 0);
        q.play_next(tracks(&["x"]).pop().unwrap());
        assert_eq!(ids(&q), vec!["a", "x", "b"]);
        assert_eq!(q.advance().unwrap().id, "x");
    }

    #[test]
    fn enqueue_vai_pro_fim() {
        let mut q = Queue::default();
        q.set(tracks(&["a"]), 0);
        q.enqueue(tracks(&["z"]).pop().unwrap());
        assert_eq!(ids(&q), vec!["a", "z"]);
    }

    #[test]
    fn fila_vazia_nao_estoura() {
        let mut q = Queue::default();
        q.set(vec![], 5);
        assert!(q.current().is_none());
        assert!(q.advance().is_none());
        assert!(q.peek_next().is_none());
        assert_eq!(q.len(), 0);
    }
}
