# Génère une voix de test (SAPI de Windows → WAV 16 kHz mono PCM 16 bits)
# pour les essais de débit du streaming sans microphone. Répété et
# déterministe : même phrase à chaque appel.
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Speech
$dir = Join-Path $env:APPDATA "Plume\test"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$wav = Join-Path $dir "voix.wav"

$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(
    16000,
    [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
    [System.Speech.AudioFormat.AudioChannel]::Mono)
$synth.SetOutputToWaveFile($wav, $fmt)
$synth.Rate = 0
# Phrase longue (~10 s) : le débit streaming se mesure sur la durée.
$synth.Speak(
    "Bonjour, je teste la dictee vocale locale en francais avec Plume. " +
    "Cette phrase longue sert a mesurer la latence du texte en direct, " +
    "du premier mot jusqu'a la fin de la prise.")
$synth.Dispose()

$item = Get-Item $wav
"OK : $wav ($([math]::Round($item.Length / 1kb, 1)) Ko)"
