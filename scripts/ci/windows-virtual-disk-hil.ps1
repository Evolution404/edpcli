$ErrorActionPreference = "Stop"

$vhd = Join-Path $env:RUNNER_TEMP "edpcli-virtual-hil.vhdx"
$diskpartCreate = Join-Path $env:RUNNER_TEMP "edpcli-create-vhd.txt"
$diskpartDetach = Join-Path $env:RUNNER_TEMP "edpcli-detach-vhd.txt"

$usedLetters = @(Get-PSDrive -PSProvider FileSystem | ForEach-Object { $_.Name.ToUpperInvariant() })
$letter = @("R", "S", "T", "U", "V", "W", "X", "Y", "Z") |
    Where-Object { $_ -notin $usedLetters } |
    Select-Object -First 1
if (-not $letter) {
    throw "No free drive letter for virtual-disk HIL"
}

function Invoke-DiskPartScript([string] $Path, [string[]] $Lines) {
    [System.IO.File]::WriteAllLines($Path, $Lines, [System.Text.Encoding]::ASCII)
    $output = & diskpart.exe /s $Path 2>&1
    if ($LASTEXITCODE -ne 0) {
        $output | Write-Host
        throw "diskpart failed with exit code $LASTEXITCODE"
    }
}

function Detach-HilVhd {
    if (Test-Path $vhd) {
        try {
            Invoke-DiskPartScript $diskpartDetach @(
                "select vdisk file=`"$vhd`"",
                "detach vdisk",
                "exit"
            )
        } catch {
            Write-Warning $_
        }
    }
}

try {
    Remove-Item $vhd -Force -ErrorAction SilentlyContinue
    Invoke-DiskPartScript $diskpartCreate @(
        "create vdisk file=`"$vhd`" maximum=128 type=expandable",
        "select vdisk file=`"$vhd`"",
        "attach vdisk",
        "convert mbr",
        "create partition primary",
        "format fs=ntfs quick label=EDPCLI_HIL",
        "assign letter=$letter",
        "exit"
    )

    $image = Get-DiskImage -ImagePath $vhd
    $disk = $image | Get-Disk
    if (-not $disk) {
        throw "Unable to resolve VHD to a Windows disk"
    }
    $rawPath = "\\.\PhysicalDrive$($disk.Number)"
    $markerPath = "${letter}:\marker.txt"
    "edpcli-virtual-hil" | Set-Content -Encoding ascii $markerPath

    $env:EDPCLI_VIRTUAL_DISK_PATH = $rawPath
    $env:CARGO_TARGET_DIR = Join-Path $env:RUNNER_TEMP "edpcli-virtual-hil-target"
    cargo test --features ci-virtual-disk --test virtual_disk_hil -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) {
        throw "Windows virtual-disk Rust HIL failed"
    }

    # 强制 detach/reattach，排除同一个 raw handle 或缓存造成的假阳性，并验证原始分区表、
    # NTFS 和文件内容在 LBA0-12 roundtrip 后仍可恢复。
    Detach-HilVhd
    Mount-DiskImage -ImagePath $vhd | Out-Null
    $disk = Get-DiskImage -ImagePath $vhd | Get-Disk
    $partition = Get-Partition -DiskNumber $disk.Number |
        Where-Object { $_.Size -gt 1MB } |
        Select-Object -First 1
    if (-not $partition) {
        throw "Restored VHD partition not found"
    }
    if (-not $partition.DriveLetter) {
        $partition | Set-Partition -NewDriveLetter $letter
        $partition = Get-Partition -DiskNumber $disk.Number -PartitionNumber $partition.PartitionNumber
    }
    $restoredMarker = "$($partition.DriveLetter):\marker.txt"
    $marker = (Get-Content $restoredMarker -Raw).Trim()
    if ($marker -ne "edpcli-virtual-hil") {
        throw "Filesystem marker was not preserved after raw roundtrip"
    }

    Write-Host "Windows virtual-disk HIL PASS: $rawPath"
}
finally {
    Detach-HilVhd
    Remove-Item $vhd -Force -ErrorAction SilentlyContinue
    Remove-Item $diskpartCreate, $diskpartDetach -Force -ErrorAction SilentlyContinue
}
