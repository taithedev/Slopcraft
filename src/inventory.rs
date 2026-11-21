use crate::{
    blocks::{BlockId, block_def, catalog},
    items::item_defs,
    render::{CatalogSlot, Slot},
};

pub const PAGE_SIZE: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Entry {
    Block(BlockId),
    Item(usize),
}

impl Entry {
    fn key(self) -> &'static str {
        match self {
            Self::Block(id) => block_def(id).key.as_str(),
            Self::Item(index) => item_defs()[index].key.as_str(),
        }
    }

    pub fn slot(self) -> Slot<'static> {
        match self {
            Self::Block(id) => {
                let block = block_def(id);
                Slot {
                    name: block.name.as_str(),
                    layer: block.top,
                    item: false,
                }
            }
            Self::Item(index) => {
                let item = &item_defs()[index];
                Slot {
                    name: item.name.as_str(),
                    layer: item.layer,
                    item: true,
                }
            }
        }
    }

    fn card(self) -> CatalogSlot<'static> {
        let slot = self.slot();
        let category = match self {
            Self::Block(id) => block_def(id).category.as_str(),
            Self::Item(index) => item_defs()[index].category.as_str(),
        };
        CatalogSlot {
            name: slot.name,
            category,
            layer: slot.layer,
            item: slot.item,
        }
    }
}

#[derive(Default)]
pub struct Inventory {
    pub open: bool,
    pub items: bool,
    pub query: String,
    selected: usize,
    matches: Vec<Entry>,
}

impl Inventory {
    pub fn open(&mut self) {
        self.open = true;
        self.refresh();
    }

    pub fn switch_tab(&mut self) {
        self.items = !self.items;
        self.query.clear();
        self.refresh();
    }

    pub fn search(&mut self, text: &str) {
        for ch in text
            .chars()
            .filter(|ch| ch.is_ascii_graphic() || *ch == ' ')
        {
            if self.query.len() < 48 {
                self.query.push(ch);
            }
        }
        self.refresh();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.refresh();
    }

    pub fn navigate(&mut self, delta: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.matches.len().saturating_sub(1));
    }

    pub fn choose_cell(&mut self, local: usize) -> Option<Entry> {
        let index = self.page() * PAGE_SIZE + local;
        let entry = self.matches.get(index).copied()?;
        self.selected = index;
        Some(entry)
    }

    pub fn selection(&self) -> Option<Entry> {
        self.matches.get(self.selected).copied()
    }

    pub fn page(&self) -> usize {
        self.selected / PAGE_SIZE
    }

    pub fn local_selected(&self) -> usize {
        self.selected % PAGE_SIZE
    }

    pub fn pages(&self) -> usize {
        self.matches.len().div_ceil(PAGE_SIZE).max(1)
    }

    pub fn cards(&self) -> Vec<CatalogSlot<'static>> {
        self.matches
            .iter()
            .skip(self.page() * PAGE_SIZE)
            .take(PAGE_SIZE)
            .map(|entry| entry.card())
            .collect()
    }

    fn refresh(&mut self) {
        let entries: Vec<Entry> = if self.items {
            (0..item_defs().len()).map(Entry::Item).collect()
        } else {
            catalog().iter().copied().map(Entry::Block).collect()
        };
        let query = self.query.to_ascii_lowercase();
        self.matches = entries
            .into_iter()
            .filter(|entry| {
                let card = entry.card();
                card.name.to_ascii_lowercase().contains(&query)
                    || card.category.to_ascii_lowercase().contains(&query)
                    || entry.key().contains(&query)
            })
            .collect();
        self.selected = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_and_pages_expose_the_full_catalog() {
        let mut inventory = Inventory::default();
        inventory.open();
        assert!(inventory.pages() >= 7);
        inventory.navigate(PAGE_SIZE as isize);
        assert_eq!(inventory.page(), 1);
        inventory.search("stone");
        assert!(
            inventory
                .cards()
                .iter()
                .all(|card| card.name.to_ascii_lowercase().contains("stone")
                    || card.category.to_ascii_lowercase().contains("stone"))
        );
        inventory.switch_tab();
        assert_eq!(inventory.matches.len(), 75);
        inventory.search("absent item query");
        assert_eq!(inventory.selection(), None);
        assert_eq!(inventory.choose_cell(0), None);
    }
}
