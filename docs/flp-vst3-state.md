# Restoring VST3 state from FL projects

FL Studio 25.2 stores VST3 state inside wrapper record 53 with an 80-byte
envelope and length-prefixed records. Record 3 contains component state;
record 4 contains a count and a table of parameter IDs. Passing the whole
envelope to the component loses the saved state.

The importer recognizes the observed version-1 envelope, checks all record
lengths, and puts component state into the host's VST3 preset container.
Unknown records remain in the project. When an imported instance is saved,
the component stream is written into the original envelope rather than replacing it
with a different container. The parameter table, original GUID bytes, and
unknown records keep their bytes.

Malformed envelopes and plugins that refuse saved state are reported as failed
restorations. They do not run at defaults and overwrite the project's saved state.

The layout was checked against an FL 25.2.5 file containing Omnisphere and
Synthesizer V Studio 2. Other envelope versions remain unsupported.
The component/controller distinction follows the
[VST3 persistence contract](https://steinbergmedia.github.io/vst3_dev_portal/pages/FAQ/Persistence.html).
