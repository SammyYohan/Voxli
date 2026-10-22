# Generates a test voice (Windows SAPI → 16 kHz mono 16-bit PCM WAV)
# for streaming throughput tests without a microphone. Repeatable and
# deterministic: same sentence on every call.
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Speech
$dir = Join-Path $env:APPDATA "Voxli\test"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$wav = Join-Path $dir "voix.wav"

$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(
    16000,
    [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
    [System.Speech.AudioFormat.AudioChannel]::Mono)
$synth.SetOutputToWaveFile($wav, $fmt)
$synth.Rate = 0
# Long sentence (~10 s): streaming throughput is measured over duration.
$synth.Speak(
    "Bonjour, je teste la dictee vocale locale en francais avec Voxli. " +
    "Cette phrase longue sert a mesurer la latence du texte en direct, " +
    "du premier mot jusqu'a la fin de la prise.")
$synth.Dispose()

$item = Get-Item $wav
"OK : $wav ($([math]::Round($item.Length / 1kb, 1)) Ko)"
