macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            serde::Deserialize,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            serde::Serialize,
        )]
        pub struct $name(u16);

        impl $name {
            pub const fn new(raw: u16) -> Self {
                Self(raw)
            }

            pub const fn raw(self) -> u16 {
                self.0
            }

            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

id_type!(ItemId);
id_type!(FluidId);
id_type!(RecipeId);
id_type!(EntityPrototypeId);
id_type!(TileId);
id_type!(TechnologyId);
