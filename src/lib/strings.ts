// Every user-facing string lives here (design_system rule 10), so i18n can be
// added later without hunting literals.

export const strings = {
  app: {
    name: "Mask",
    settings: "Settings",
  },
  status: {
    stopped: "Stopped",
    starting: "Starting",
    running: "Live",
    error: "Error",
  },
  devices: {
    panelTitle: "Devices",
    microphone: "Microphone",
    output: "Output (virtual mic)",
    monitor: "Monitor",
    monitorOff: "Off",
    start: "Start",
    stop: "Stop",
    rescan: "Re-scan devices",
    noInputSelected: "Pick a microphone first",
    inputMeter: "In",
    outputMeter: "Out",
    noDevices: "No audio devices found",
    cableMissingBanner: "No virtual cable installed. Calls will not hear your voice.",
    cableMissingAction: "Open setup guide",
  },
  effects: {
    panelTitle: "Effects",
    custom: "Custom",
    pitch: "Pitch",
    formant: "Formant",
    semitonesUnit: "st",
    saveAsPreset: "Save as preset",
    deletePreset: "Delete",
    confirmDelete: "Click again to delete",
  },
  soundboard: {
    panelTitle: "Soundboard",
    import: "Import sounds",
    stopAll: "Stop all",
    empty: "No sounds yet",
    emptyHint: "Import audio files and play them into your call",
    rename: "Rename",
    volume: "Volume",
    delete: "Delete",
    broken: "File missing. Re-import this sound.",
    importFilterName: "Audio",
  },
  onboarding: {
    title: "Route your voice into calls",
    explain:
      "Mask needs a virtual audio cable so Discord, Zoom and friends can use your modified voice as a microphone. It is a one-time install, made by VB-Audio.",
    explainAlt: "Prefer open source? Virtual-Audio-Driver (MIT) also works.",
    downloadVbCable: "Download VB-Cable",
    downloadAlt: "Download Virtual-Audio-Driver",
    waiting: "After installing (a reboot may be needed), come back here.",
    rescan: "Check again",
    detected: "Cable detected!",
    detectedBody: "Mask will send your voice to it. One last step on the call side:",
    callAppGuide:
      'In Discord / Zoom, set the microphone to "CABLE Output (VB-Audio Virtual Cable)".',
    testHint: "Speak. If the Out meter moves, the route works.",
    skip: "Skip for now",
    next: "Next",
    done: "Finish",
  },
  errors: {
    pipelineStart: "Could not start the microphone.",
    showDetail: "Details",
  },
  links: {
    vbCable: "https://vb-audio.com/Cable/",
    virtualAudioDriver: "https://github.com/VirtualDrivers/Virtual-Audio-Driver/releases",
  },
} as const;
