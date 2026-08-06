use super::*;

/// Where a finished craft's item products go.
///
/// This is the whole of what separates a rocket silo from an assembling
/// machine. Both consume the same ingredients against the same recipe over the
/// same accumulated ticks; one puts the result in slots an inserter can reach,
/// and the other raises a counter toward a rocket. Keeping the difference in one
/// enum is what lets [`ItemCraft`] below be the single crafting cycle rather
/// than two that have to be kept in step.
pub(super) enum CraftProducts<'a> {
    /// Output slots holding finished items until something takes them.
    Inventory(&'a mut Inventory),
    /// A rocket under construction. Parts are counted rather than stored, and
    /// the rocket standing on the pad is the reason a full silo stops: there is
    /// nowhere for the next part to go until it leaves.
    ///
    /// One craft is one part. That is a catalog invariant rather than an
    /// assumption made here — loading rejects a `RocketBuilding` recipe that
    /// yields anything but a single unit — and it is what keeps this counter
    /// agreeing with the production statistics recorded from the same craft.
    RocketParts {
        completed: &'a mut u32,
        per_rocket: u32,
    },
}

impl CraftProducts<'_> {
    fn has_room_for(
        &self,
        catalog: &PrototypeCatalog,
        products: &[factory_data::ItemAmount],
        copies: u64,
    ) -> bool {
        match self {
            Self::Inventory(inventory) => {
                assembler_output_can_accept_copies(catalog, inventory, products, copies)
            }
            // Room is asked about the rocket, not about the parts: a whole
            // rocket blocks the silo, and anything short of one has room for
            // another part however many productivity copies are due.
            Self::RocketParts {
                completed,
                per_rocket,
            } => **completed < *per_rocket,
        }
    }

    /// Stores `copies` of `products` and reports how many landed.
    ///
    /// An inventory takes them all — the caller checked the room first. A rocket
    /// takes what fits under `per_rocket` and drops the rest, which only differs
    /// when productivity modules make the last craft of a rocket overshoot.
    fn store(
        &mut self,
        catalog: &PrototypeCatalog,
        products: &[factory_data::ItemAmount],
        copies: u64,
    ) -> u64 {
        match self {
            Self::Inventory(inventory) => {
                for _ in 0..copies {
                    for product in products {
                        inventory
                            .insert(catalog, product.item, product.amount)
                            .expect("output capacity was checked before completion");
                    }
                }
                copies
            }
            Self::RocketParts {
                completed,
                per_rocket,
            } => {
                debug_assert!(
                    matches!(products, [product] if product.amount == 1),
                    "catalog loading rejects a rocket-building recipe that is not one part a craft"
                );
                let stored = copies.min(u64::from(per_rocket.saturating_sub(**completed)));
                **completed = completed.saturating_add(stored.min(u64::from(u32::MAX)) as u32);
                stored
            }
        }
    }
}

/// One machine's item-side crafting cycle: the ingredients it draws on and
/// where its products end up.
///
/// This is the shared half of the assembler and rocket silo tick loops. It
/// deliberately holds no progress, energy, or fluid state: those differ between
/// the two callers (only the assembler has fluid boxes) and each loop keeps its
/// own, so the piece that is shared is exactly the piece that is the same.
pub(super) struct ItemCraft<'a> {
    pub(super) input_inventory: &'a mut Inventory,
    pub(super) products: CraftProducts<'a>,
}

impl ItemCraft<'_> {
    /// Whether one cycle's ingredients are in stock and `copies` of its products
    /// would fit.
    pub(super) fn can_craft(
        &self,
        catalog: &PrototypeCatalog,
        recipe: &factory_data::RecipePrototype,
        copies: u64,
    ) -> bool {
        assembler_has_ingredients(self.input_inventory, &recipe.ingredients)
            && self
                .products
                .has_room_for(catalog, &recipe.products, copies)
    }

    /// Consumes one cycle's ingredients and stores `copies` of its products,
    /// reporting how many copies landed.
    ///
    /// Only call this after [`Self::can_craft`] answered `true` for the same
    /// `copies`; the insertions below treat that check as already made.
    pub(super) fn complete(
        &mut self,
        catalog: &PrototypeCatalog,
        recipe: &factory_data::RecipePrototype,
        copies: u64,
    ) -> u64 {
        for ingredient in &recipe.ingredients {
            self.input_inventory
                .remove(ingredient.item, ingredient.amount)
                .expect("ingredients were checked before completion");
        }
        self.products.store(catalog, &recipe.products, copies)
    }
}

/// Records one finished item craft against production statistics and onboarding
/// progress.
///
/// Free-standing rather than a [`MachineTickContext`] method because the recipe
/// slices the callers pass borrow the prototype catalog out of the same context:
/// taking `&mut self` here would conflict with the borrow that produced
/// `recipe`, which is why the tick loops reach for individual fields.
pub(super) fn record_item_craft(
    statistics: &mut StatisticsContext<'_>,
    onboarding: &mut OnboardingProgress,
    base: &factory_data::BasePrototypeIds,
    recipe: &factory_data::RecipePrototype,
    copies: u64,
) {
    for ingredient in &recipe.ingredients {
        statistics.record_item_consumed(ingredient.item, u64::from(ingredient.amount));
    }
    for product in &recipe.products {
        let produced = u64::from(product.amount).saturating_mul(copies);
        statistics.record_item_produced(product.item, produced);
        onboarding.record_item_produced(base, product.item, produced);
    }
}
