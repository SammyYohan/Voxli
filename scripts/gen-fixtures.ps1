# Generation of the audio fixtures (tests/fixtures/audio).
#
# Writes deterministic 16 kHz mono PCM16 WAVs (no binary file tracked
# in the repository). These fixtures serve the manual acceptance tests and future
# integration tests of the real engine: each file has usefully
# identifiable content (docs/06 §1).
#
# Performance: writes into pre-sized arrays — never
# one-by-one additions via List[T] (10x slower under PowerShell).
#
# Run: powershell -ExecutionPolicy Bypass -File scripts/gen-fixtures.ps1

param([string] $Out = 'tests/fixtures/audio')

$ErrorActionPreference = 'Stop'
$rate = 16000

function New-Sine {
    param([double] $Freq, [int] $Count, [int] $SampleRate, [double] $Amp = 0.4)
    $samples = New-Object 'float[]' $Count
    for ($i = 0; $i -lt $Count; $i++) {
        $samples[$i] = [float]($Amp * [math]::Sin((2 * [math]::PI * $Freq * $i) / $SampleRate))
    }
    return $samples
}

function ConvertTo-Pcm16Bytes {
    param([float[]] $Samples)
    $n = $Samples.Length
    $bytes = New-Object 'byte[]' ($n * 2)
    for ($i = 0; $i -lt $n; $i++) {
        $v = [double]$Samples[$i]
        if ($v -gt 1.0) { $v = 1.0 } elseif ($v -lt -1.0) { $v = -1.0 }
        $s = [Int16][math]::Round($v * 32767)
        [BitConverter]::GetBytes($s).CopyTo($bytes, 2 * $i)
    }
    return $bytes
}

function Write-Wav {
    param([string] $Path, [float[]] $Samples, [int] $SampleRate)

    $pcm = ConvertTo-Pcm16Bytes $Samples
    $dataLen = $pcm.Length

    $buf = New-Object 'byte[]' (44 + $dataLen)
    $buf[0] = 0x52; $buf[1] = 0x49; $buf[2] = 0x46; $buf[3] = 0x46   # 'RIFF'
    [BitConverter]::GetBytes([UInt32](36 + $dataLen)).CopyTo($buf, 4)
    $buf[8] = 0x57; $buf[9] = 0x41; $buf[10] = 0x56; $buf[11] = 0x45 # 'WAVE'
    $buf[12] = 0x66; $buf[13] = 0x6D; $buf[14] = 0x74; $buf[15] = 0x20 # 'fmt '
    [BitConverter]::GetBytes([UInt32]16).CopyTo($buf, 16)
    [BitConverter]::GetBytes([UInt16]1).CopyTo($buf, 20)   # PCM
    [BitConverter]::GetBytes([UInt16]1).CopyTo($buf, 22)   # mono
    [BitConverter]::GetBytes([UInt32]$SampleRate).CopyTo($buf, 24)
    [BitConverter]::GetBytes([UInt32]($SampleRate * 2)).CopyTo($buf, 28)
    [BitConverter]::GetBytes([UInt16]2).CopyTo($buf, 32)
    [BitConverter]::GetBytes([UInt16]16).CopyTo($buf, 34)
    $buf[36] = 0x64; $buf[37] = 0x61; $buf[38] = 0x74; $buf[39] = 0x61 # 'data'
    [BitConverter]::GetBytes([UInt32]$dataLen).CopyTo($buf, 40)
    $pcm.CopyTo($buf, 44)

    [System.IO.File]::WriteAllBytes($Path, $buf)
}

function Concat {
    param([float[][]] $Parts)
    $total = 0
    foreach ($p in $Parts) { $total += $p.Length }
    $acc = New-Object 'float[]' $total
    $off = 0
    foreach ($p in $Parts) { $p.CopyTo($acc, $off); $off += $p.Length }
    return $acc
}

New-Item -ItemType Directory -Force -Path $Out | Out-Null
Write-Host "Génération des fixtures dans $Out ..."

# 1. Pure silence: the VAD must answer "silence", the window produces nothing.
Write-Wav (Join-Path $Out 'silence-2s.wav') (New-Object 'float[]' (2 * $rate)) $rate

# 2. "Speech-like" tone (three mid-band components, fundamental
#    220 Hz): the VAD must let it through; the real engine must read an utterance in it.
$voix = Concat @(
    (New-Sine -Freq 220.0 -Count $rate -SampleRate $rate -Amp 0.28),
    (New-Sine -Freq 440.0 -Count $rate -SampleRate $rate -Amp 0.28),
    (New-Sine -Freq 880.0 -Count $rate -SampleRate $rate -Amp 0.28)
)
Write-Wav (Join-Path $Out 'voix-synthetique-3s.wav') $voix $rate

# 3. Take too short: must be rejected by minSpeechMs = 250 (RF-18).
Write-Wav (Join-Path $Out 'short-100ms.wav') (New-Sine -Freq 440.0 -Count ([int]($rate * 0.1)) -SampleRate $rate) $rate

# 4. Approximated keyboard noise (random burst + silence): the VAD must
#    answer Silence; the pipeline inserts nothing (RF-18).
$nNoise = [int]($rate * 0.05)
$rand = New-Object System.Random(42)
$noise = New-Object 'float[]' $rate
for ($i = 0; $i -lt $nNoise; $i++) { $noise[$i] = [float](($rand.NextDouble() - 0.5) * 0.6) }
Write-Wav (Join-Path $Out 'clic-clavier-1s.wav') $noise $rate

# 5. Long take: max-duration and latency exercise (docs/06 §4).
Write-Wav (Join-Path $Out 'long-10s.wav') (New-Sine -Freq 300.0 -Count (10 * $rate) -SampleRate $rate -Amp 0.3) $rate

Write-Host 'Fixtures écrites :'
Get-ChildItem $Out | ForEach-Object { Write-Output ('  {0,-28} {1,8:F0} Ko' -f $_.Name, ($_.Length / 1KB)) }
