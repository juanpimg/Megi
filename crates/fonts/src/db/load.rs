//! Font loading, alias registration and face-data/building helpers.

use super::*;

/// Re-assert the deterministic coarse generic names. fontdb's
/// `load_system_fonts()` reads fontconfig aliases and overwrites them with
/// host-specific faces ("Nimbus Sans Narrow", "Standard Symbols PS" on
/// CachyOS); CSS Fonts 4 §5.2 requires a browser's own documented fallback
/// instead. Call on construction and after every host font scan.
pub(super) fn pin_generic_families(db: &mut Database) {
    db.set_sans_serif_family(GENERIC_SANS_FAMILY);
    db.set_serif_family(GENERIC_SERIF_FAMILY);
    db.set_monospace_family(GENERIC_MONOSPACE_FAMILY);
    db.set_cursive_family(GENERIC_CURSIVE_FAMILY);
    db.set_fantasy_family(GENERIC_FANTASY_FAMILY);
}

impl FontDatabase {
    /// Deterministic digest of all loaded faces (identities plus face bytes). Used by the
    /// render layout cache to avoid reusing metrics computed with different fonts (#1218).
    pub fn identity_digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut mix = |value: u64| {
            hash ^= value;
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        };
        mix(self.order.len() as u64);
        for id in &self.order {
            if let Some(face) = self.db.face(*id) {
                for byte in face.post_script_name.as_bytes() {
                    mix(u64::from(*byte));
                }
                for (name, _) in &face.families {
                    for byte in name.as_bytes() {
                        mix(u64::from(*byte));
                    }
                }
                mix(face.weight.0 as u64);
                mix((face.style as u8) as u64);
                mix((face.stretch as u8) as u64);
            }
            if let Ok(data) = self.face_data(*id) {
                for byte in data.iter() {
                    mix(u64::from(*byte));
                }
            }
        }
        hash
    }

    /// Loads the deterministic bundled fonts (DejaVu Sans regular + bold) from
    /// `assets/` via `include_bytes!`.
    ///
    /// This is the recommended constructor for **reproducible rendering and
    /// tests**: results do not depend on which fonts are installed on the host.
    /// No system font is loaded.
    pub fn load_bundled() -> Result<Self, FontError> {
        let mut database = Self::new();
        for (_name, bytes) in BUNDLED_FONTS {
            database.load_font_data(bytes.to_vec())?;
        }
        Ok(database)
    }

    /// Infallible variant of [`FontDatabase::load_bundled`]: the bundled bytes are
    /// validated at compile time, so this degrades to an empty database instead of
    /// failing if they could ever be corrupt.
    pub fn new_with_bundled_fonts() -> Self {
        Self::load_bundled().unwrap_or_else(|_| Self::new())
    }

    /// Loads the deterministic bundled fonts first (preserving bundled DejaVu priority
    /// for Latin and CSS generic families), then loads installed system fonts so
    /// per-glyph fallback can render CJK, Arabic, Hebrew, and emoji scripts.
    pub fn new_with_bundled_and_system_fonts() -> Self {
        let mut db = Self::new_with_bundled_fonts();
        db.load_system_fonts();
        db
    }

    /// Creates an empty database (no system fonts, no loaded bytes).
    pub fn new() -> Self {
        let mut db = Database::new();
        pin_generic_families(&mut db);
        Self {
            db,
            cache: Mutex::new(HashMap::new()),
            fallback_cache: Mutex::new(HashMap::new()),
            order: Vec::new(),
            aliases: HashMap::new(),
            loaded_hashes: HashMap::new(),
            loaded_bytes: 0,
            loaded_faces: 0,
            system_loaded: false,
        }
    }

    /// Creates a database pre-loaded with the fonts installed on the system.
    ///
    /// This scans the standard font directories and is the only potentially slow
    /// constructor (~tens of ms). It never fails: an empty result simply means the
    /// environment has no discoverable fonts.
    pub fn from_system() -> Self {
        let mut database = Self::new();
        database.load_system_fonts();
        database
    }

    /// Loads the system font directories if they were not loaded yet.
    pub fn load_system_fonts(&mut self) {
        if self.system_loaded {
            return;
        }
        self.db.load_system_fonts();
        // fontdb applies fontconfig aliases during the scan; restore Megi's
        // documented generic families so the host cannot change CSS rendering.
        pin_generic_families(&mut self.db);
        self.system_loaded = true;
        self.refresh_order();
        self.fallback_cache_lock().clear();
    }

    /// Loads one font file (TTF/OTF/TTC) from disk.
    pub fn load_font_file(&mut self, path: impl AsRef<Path>) -> Result<(), FontError> {
        let path = path.as_ref();
        // Reject non-regular files (devices, FIFOs, ...) and cap the read size.
        let metadata = std::fs::metadata(path).map_err(|err| FontError::Io {
            path: path.display().to_string(),
            reason: err.to_string(),
        })?;
        if !metadata.is_file() {
            return Err(FontError::Io {
                path: path.display().to_string(),
                reason: "not a regular file".to_string(),
            });
        }
        const MAX_FONT_FILE_BYTES: u64 = 64 * 1024 * 1024;
        if metadata.len() > MAX_FONT_FILE_BYTES {
            return Err(FontError::Io {
                path: path.display().to_string(),
                reason: format!("font file too large ({} bytes)", metadata.len()),
            });
        }
        let bytes = std::fs::read(path).map_err(|err| FontError::Io {
            path: path.display().to_string(),
            reason: err.to_string(),
        })?;
        self.load_font_data(bytes)
    }

    pub(crate) fn load_font_data_inner(
        &mut self,
        data: Vec<u8>,
    ) -> Result<Vec<fontdb::ID>, FontError> {
        if data.is_empty() {
            return Err(FontError::InvalidFont {
                reason: "empty font data".to_string(),
            });
        }
        let key = (data.len(), fnv1a_64(&data));
        if let Some(buckets) = self.loaded_hashes.get(&key) {
            let cache = self.cache_lock();
            for existing_ids in buckets {
                if let Some(first_id) = existing_ids.first()
                    && let Some(cached) = cache.get(first_id)
                    && cached.as_slice() == data.as_slice()
                {
                    return Ok(existing_ids.clone());
                }
            }
        }
        if self.loaded_faces >= MAX_LOADED_FONT_FACES {
            return Err(FontError::InvalidFont {
                reason: format!("font face limit reached ({MAX_LOADED_FONT_FACES})"),
            });
        }
        if self.loaded_bytes.saturating_add(data.len()) > MAX_LOADED_FONT_BYTES {
            return Err(FontError::InvalidFont {
                reason: format!("total loaded font bytes exceed {MAX_LOADED_FONT_BYTES} byte cap"),
            });
        }
        if let Err(err) = ttf_parser::Face::parse(&data, 0) {
            return Err(FontError::InvalidFont {
                reason: err.to_string(),
            });
        }
        let byte_len = data.len();
        let shared: Arc<Vec<u8>> = Arc::new(data);
        let ids: Vec<fontdb::ID> = self
            .db
            .load_font_source(Source::Binary(
                shared.clone() as Arc<dyn AsRef<[u8]> + Send + Sync>
            ))
            .to_vec();
        if ids.is_empty() {
            return Err(FontError::InvalidFont {
                reason: "fontdb registered no face for the supplied data".to_string(),
            });
        }
        {
            let mut cache = self.cache_lock();
            for id in ids.iter() {
                cache.insert(*id, shared.clone());
            }
        }
        self.order.extend(ids.iter().copied());
        self.loaded_bytes = self.loaded_bytes.saturating_add(byte_len);
        self.loaded_faces = self.loaded_faces.saturating_add(ids.len());
        self.loaded_hashes.entry(key).or_default().push(ids.clone());
        self.fallback_cache_lock().clear();
        Ok(ids)
    }

    /// Loads font bytes (a single face or a font collection) into the database.
    ///
    /// The data is validated with `ttf-parser` before being registered; malformed
    /// input returns [`FontError::InvalidFont`] and leaves the database unchanged.
    pub fn load_font_data(&mut self, data: Vec<u8>) -> Result<(), FontError> {
        self.load_font_data_inner(data).map(|_| ())
    }

    /// Loads raw sfnt (TTF/OTF) bytes under a CSS family **alias**.
    ///
    /// This is the `@font-face` entry point: the CSS `font-family` descriptor is the
    /// alias (it may differ from the family in the font's `name` table), and
    /// [`Self::font`]/[`Self::measure_text`]/[`Self::shape`] resolve it in priority
    /// order with the rest of the CSS family list. WOFF2 bytes must be decoded first
    /// with [`crate::decode_web_font`].
    ///
    /// Malformed data returns [`FontError::InvalidFont`] and leaves the database
    /// unchanged.
    pub fn load_font_data_family(
        &mut self,
        family_alias: &str,
        bytes_ttf: Vec<u8>,
    ) -> Result<(), FontError> {
        let alias = family_alias.trim();
        if alias.is_empty() {
            return Err(FontError::InvalidFont {
                reason: "empty @font-face family alias".to_string(),
            });
        }
        let ids = self.load_font_data_inner(bytes_ttf)?;
        if ids.is_empty() {
            return Err(FontError::InvalidFont {
                reason: "fontdb registered no face for the supplied @font-face data".to_string(),
            });
        }
        self.aliases
            .entry(alias.to_ascii_lowercase())
            .or_default()
            .extend(ids);
        self.fallback_cache_lock().clear();
        Ok(())
    }

    /// True if the family was registered through [`Self::load_font_data_family`].
    pub fn has_alias(&self, family: &str) -> bool {
        self.aliases.contains_key(&family.to_ascii_lowercase())
    }

    pub(crate) fn refresh_order(&mut self) {
        self.order = self.db.faces().map(|face| face.id).collect();
    }

    /// Number of registered faces.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// True when no face is registered.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Sorted, deduplicated English family names available in the database.
    pub fn family_names(&self) -> Vec<String> {
        let mut names: BTreeSet<String> = self
            .order
            .iter()
            .filter_map(|id| self.db.face(*id))
            .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
            .collect();
        names.extend(self.aliases.keys().cloned());
        names.into_iter().collect()
    }

    /// True if a face with this family name (ASCII case-insensitive per CSS Fonts 3)
    /// or an `@font-face` alias is registered.
    pub fn has_family(&self, family: &str) -> bool {
        if self.aliases.contains_key(&family.to_ascii_lowercase()) {
            return true;
        }
        self.order
            .iter()
            .filter_map(|id| self.db.face(*id))
            .any(|face| {
                face.families
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case(family))
            })
    }

    pub(crate) fn cache_lock(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<fontdb::ID, Arc<Vec<u8>>>> {
        match self.cache.lock() {
            Ok(guard) => guard,
            // A poisoned lock only means another thread panicked while holding it;
            // the cache itself is still consistent, so reuse the data.
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) fn fallback_cache_lock(&self) -> std::sync::MutexGuard<'_, FallbackChainMap> {
        match self.fallback_cache.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) fn face_data(&self, id: fontdb::ID) -> Result<Arc<Vec<u8>>, FontError> {
        if let Some(data) = self.cache_lock().get(&id).cloned() {
            return Ok(data);
        }
        let data = self
            .db
            .with_face_data(id, |bytes, _index| Arc::new(bytes.to_vec()))
            .ok_or_else(|| FontError::InvalidFont {
                reason: "fontdb has no data for the requested face".to_string(),
            })?;
        self.cache_lock().insert(id, data.clone());
        Ok(data)
    }

    pub(crate) fn build_font(&self, id: fontdb::ID) -> Result<Font, FontError> {
        let face = self.db.face(id).ok_or_else(|| FontError::InvalidFont {
            reason: "fontdb face id is no longer registered".to_string(),
        })?;
        let info = FontRefInfo {
            family: face
                .families
                .first()
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| face.post_script_name.clone()),
            post_script_name: face.post_script_name.clone(),
            weight: face.weight.0,
            style: face.style.into(),
            stretch: face.stretch,
            monospaced: face.monospaced,
        };
        let index = face.index;
        let data = self.face_data(id)?;
        Font::new(data, index, info)
    }
}
