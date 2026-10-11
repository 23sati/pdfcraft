# Fork FrancescoClemeno/pdfcraft

Questo fork è PdfCraft ([storytold/pdfcraft](https://github.com/storytold/pdfcraft)) più alcune
modifiche personali. Si tiene aggiornato da solo e pubblica le proprie build, così l'app che scarichi
resta aggiornata **e** mantiene le modifiche.

## Modifiche del fork

| Funzione | Dove | Note |
|---|---|---|
| **Copia immagine** | Tasto destro su un'immagine della pagina ▸ *Copia immagine*; anche in *Modifica testo e immagini* (tasto destro sull'immagine selezionata) | Copia negli appunti l'immagine originale alla sua risoluzione (non uno screenshot della pagina), con la trasparenza. Immagini oltre 64 milioni di pixel: usare *Salva immagine con nome*. Le immagini che PdfCraft non sa ancora decodificare (JPEG 2000, JBIG2, CCITT) danno un messaggio d'errore. Non disponibile nella versione web. |

Codice: `Document::page_image_at` e `Document::page_image_rgba` (`crates/engine/src/lib.rs`),
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
   **`.github/workflows/fork-release.yml`**, che compila macOS (universal), Windows x64 e Linux x86_64
   e pubblica la release `vX.Y.Z` del fork.
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

Le build del fork non hanno un certificato a pagamento:

- **macOS**: dopo aver copiato PdfCraft in Applicazioni, la prima volta clic destro sull'app ▸
  *Apri* ▸ *Apri*. Se dice che è "danneggiata": `xattr -dr com.apple.quarantine /Applications/PdfCraft.app`.
- **Windows**: SmartScreen ▸ *Ulteriori informazioni* ▸ *Esegui comunque*.

## Aggiungere altre modifiche

- Lavora su un branch, poi unisci in `main`; il sync quotidiano le conserva.
- Per ridurre i conflitti con l'originale, preferisci file nuovi e modifiche piccole e localizzate
  ai file esistenti, e aggiungi le traduzioni accanto a voci esistenti (non in fondo ai file
  `crates/ui-egui/src/i18n/*.tsv`, dove anche l'originale aggiunge righe).
- Ogni testo `tl!("…")` nuovo va tradotto in tutti i cataloghi controllati dai test (vedi
  `crates/ui-egui/src/i18n/mod.rs`), altrimenti `cargo test` fallisce.
- Aggiorna la tabella qui sopra.
