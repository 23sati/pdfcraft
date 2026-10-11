# Fork FrancescoClemeno/pdfcraft

Questo fork è PdfCraft ([storytold/pdfcraft](https://github.com/storytold/pdfcraft)) più alcune
modifiche personali. Si tiene aggiornato da solo e pubblica le proprie build, così l'app che scarichi
resta aggiornata **e** mantiene le modifiche.

## Modifiche del fork

| Funzione | Dove | Note |
|---|---|---|
| **Seleziona e copia un'immagine nel formato originale** | Strumento *Seleziona*: clic su un'immagine (si evidenzia in blu), poi ⌘C / Ctrl+C. Oppure tasto destro sull'immagine ▸ *Copia immagine nel formato originale*. Esc deseleziona. Il clic singolo seleziona l'immagine anche se sopra c'è del testo (scansioni con OCR, didascalie); trascinando o con doppio clic si seleziona il testo. | Negli appunti finisce un **file**: il JPEG così com'è salvato nel PDF (nessuna ricompressione), ogni altra immagine come PNG senza perdita, con la trasparenza. Si incolla dove si incollano i file (Finder/Esplora risorse, email, chat, Word). Il file temporaneo sta nella cartella `pdfcraft-clipboard` della cartella temporanea di sistema e viene sostituito alla copia successiva. |
| **Copia immagine** (pixel) | Tasto destro su un'immagine ▸ *Copia immagine*; anche in *Modifica testo e immagini* | Copia i pixel dell'immagine alla sua risoluzione (non uno screenshot della pagina): si incolla in qualunque programma che accetta immagini (editor di immagini, browser). Oltre 64 milioni di pixel: usare *Salva immagine con nome*. |

Limiti comuni: le immagini che PdfCraft non sa ancora estrarre (JPEG 2000, JBIG2, CCITT) danno un
messaggio d'errore; i disegni vettoriali non sono immagini e non si selezionano (un gruppo di grafica, Form XObject, si seleziona se contiene un'immagine: viene copiata la più grande che contiene); nella versione web
la copia negli appunti non è disponibile.

Codice: `crates/engine/src/copy_image.rs` (trovare e estrarre l'immagine, anche dentro i Form),
`crates/ui-egui/src/image_pick.rs` (selezione e copia come file),
`PdfCraftApp::copy_page_image` (`crates/ui-egui/src/content_ui.rs`), voci di menu in
`crates/ui-egui/src/canvas.rs` e `crates/ui-egui/src/edit_text_ui.rs`.

Inoltre *Aiuto ▸ Verifica aggiornamenti* guarda le release di **questo** fork
(`crates/ui-egui/src/updates.rs`, `apps/pdfcraft/src/updates.rs`), non quelle originali.

## Come resta aggiornato

1. **`.github/workflows/fork-sync.yml`** (ogni giorno, o a mano da *Actions ▸ Fork sync ▸ Run workflow*)
   unisce `main` di storytold/pdfcraft nel `main` del fork con un merge commit: le modifiche del fork
   restano. Se c'è un conflitto non spinge nulla e apre una issue con etichetta `fork-sync` che dice
   quali file sistemare.
2. Se la versione in `Cargo.toml` è nuova (non esiste ancora la release `vX.Y.Z` nel fork), avvia
   **`.github/workflows/fork-release.yml`**, che compila l'installer Windows x64 (`.msi`) e la versione
   portatile, e pubblica la release `vX.Y.Z` del fork.
3. L'app, con *Aiuto ▸ Verifica aggiornamenti*, trova quella release e apre la pagina per scaricarla
   (PdfCraft non installa nulla da solo).

Per ricompilare la stessa versione dopo aver cambiato il codice del fork: *Actions ▸ Fork sync ▸ Run
workflow* con "Also build and publish a fork release now" spuntato (o *Actions ▸ Fork release ▸ Run
workflow*). La release con lo stesso numero viene sostituita; l'app non la segnala come nuova, va
scaricata a mano.

## Configurazione una tantum (su GitHub)

1. **Abilitare le Actions** nel fork: scheda *Actions* ▸ "I understand my workflows, go ahead and
   enable them". I workflow pianificati dei fork sono spenti finché non lo fai.
2. **Token per la sincronizzazione** (consigliato): il token predefinito non può spingere modifiche
   ai file in `.github/workflows`, quindi il sync si fermerebbe la prima volta che l'originale ne
   cambia uno. Crea un *fine-grained personal access token* (Settings ▸ Developer settings), solo
   per questo repository, con permessi *Contents*, *Workflows*, *Actions* e *Issues* in lettura e
   scrittura, e salvalo come secret del repository `FORK_SYNC_TOKEN` (*Settings ▸ Secrets and
   variables ▸ Actions*).
3. Prima release: *Actions ▸ Fork sync ▸ Run workflow* con la casella della release spuntata.

GitHub sospende i workflow pianificati dopo 60 giorni senza attività nel repository: se succede,
riattivali dalla scheda *Actions*.

## Primo avvio (build non firmate)

La build del fork non ha un certificato a pagamento:

- **Windows**: SmartScreen ▸ *Ulteriori informazioni* ▸ *Esegui comunque*.

Per avere anche macOS o Linux, copia i job corrispondenti da `.github/workflows/release.yml`
in `fork-release.yml` (senza `environment: release` e con un runner standard, es. `macos-15`).

## Aggiungere altre modifiche

- Lavora su un branch, poi unisci in `main`; il sync quotidiano le conserva.
- Per ridurre i conflitti con l'originale, preferisci file nuovi e modifiche piccole e localizzate
  ai file esistenti, e aggiungi le traduzioni accanto a voci esistenti (non in fondo ai file
  `crates/ui-egui/src/i18n/*.tsv`, dove anche l'originale aggiunge righe).
- Ogni testo `tl!("…")` nuovo va tradotto in tutti i cataloghi controllati dai test (vedi
  `crates/ui-egui/src/i18n/mod.rs`), altrimenti `cargo test` fallisce.
- Aggiorna la tabella qui sopra.
