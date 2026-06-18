#!/usr/bin/env pwsh
#Requires -Version 7.0

[CmdletBinding(PositionalBinding=$true)]
param(
	[ValidatePattern('https?://.+')]
	[Parameter(Mandatory = $false, ValueFromPipelineByPropertyName)]
	[string] $Server = $env:GITHUB_SERVER_URL ?? "https://github.com",

	[ValidateNotNullOrWhiteSpace()]
	[Parameter(Mandatory = $false, ValueFromPipelineByPropertyName)]
	[string] $Repository = $env:GITHUB_REPOSITORY,

	[Parameter(Position = 0, Mandatory = $false, ValueFromPipeline, ValueFromPipelineByPropertyName)]
	[Alias('FullName', 'Path')]
	[string] $Dir = 'target/doc',

	[Parameter(Mandatory = $false, ValueFromPipelineByPropertyName)]
	[string] $Asset = (Join-Path ($env:GITHUB_WORKSPACE ?? $PWD) 'design/A4.svg')
)

dynamicparam {
	$attributeCollection = [System.Collections.ObjectModel.Collection[System.Attribute]]::new()
	$attributeCollection.Add([System.Management.Automation.ParameterAttribute]@{ Mandatory = $false })

	$paramDictionary = [System.Management.Automation.RuntimeDefinedParameterDictionary]::new()
	$paramDictionary.Add(
		'Indent',
		[System.Management.Automation.RuntimeDefinedParameter]::new('Indent', [object], $attributeCollection)
	)
	$paramDictionary
}

begin {
	$ErrorActionPreference = 'Stop'

	$indentValue = $PSBoundParameters.ContainsKey('Indent') ? $PSBoundParameters['Indent'] : "`t"
	$indent = $indentValue -is [string] ? $indentValue : (' ' * [Math]::Max(0, [int] $indentValue))
	$actionDir = $env:GITHUB_ACTION_PATH ?? $PSScriptRoot
	$workspace = $env:GITHUB_WORKSPACE ?? (Get-Location).ProviderPath

	# Outside GitHub Actions GITHUB_REPOSITORY is unset; derive `owner/repo` from
	# the `origin` remote so the "Source on GitHub" link resolves when run locally.
	if (-not $Repository) {
		$remote = git -C $workspace remote get-url origin 2>$null
		if ($remote -match '[:/]([^/:]+/[^/]+?)(?:\.git)?/?$') { $Repository = $Matches[1] }
	}

	$manifest = Join-Path $workspace 'Cargo.toml'
	$versions = @{}
	(cargo metadata --manifest-path $manifest --format-version 1 --no-deps | ConvertFrom-Json).packages | ForEach-Object { $versions[$_.name] = $_.version }
}

process {
	$docDir = [System.IO.Path]::IsPathRooted($Dir) ? $Dir : (Join-Path $workspace $Dir)
	$asset = [System.IO.Path]::IsPathRooted($Asset) ? $Asset : (Join-Path $workspace $Asset)
	$targetDir = Split-Path -Parent $docDir
	cargo clean --doc --manifest-path $manifest --target-dir $targetDir
	cargo doc --workspace --no-deps --quiet --all-features --document-private-items --lib --bins --examples --manifest-path $manifest --target-dir $targetDir

	if (-not (Test-Path -LiteralPath $docDir -PathType Container)) {
		throw "Documentation directory not found: $docDir"
	}

	if (-not (Test-Path -LiteralPath $asset -PathType Leaf)) {
		throw "Asset not found: $asset"
	}

	if (-not $Server -or -not $Repository) {
		throw 'Server and repository are required outside GitHub Actions.'
	}

	$assetDir = Join-Path $docDir 'assets'
	New-Item -ItemType Directory -Force -Path $assetDir | Out-Null
	Copy-Item -LiteralPath $asset -Destination $assetDir

	$links = Get-ChildItem -LiteralPath $docDir -Directory |
		Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'index.html') } |
		Where-Object { $versions.ContainsKey(($_.Name -replace '_', '-')) } |
		Sort-Object Name |
		ForEach-Object {
			$name = $_.Name -replace '_', '-'
			$version = $versions[$name]
			$tag = $version ? " <span class=""ver"">$version</span>" : ''
			"$indent<li><a href=""$($_.Name)/index.html"">$name</a>$tag</li>"
		}

	$sourceUrl = "$Server/$Repository"
	$favicon = [Convert]::ToBase64String([IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $asset)))
	$css = (Get-Content -LiteralPath (Join-Path $actionDir 'index.css') -Raw).Trim()
	$html = (Get-Content -LiteralPath (Join-Path $actionDir 'index.html') -Raw).Trim()
	$sep = "`t" * 3

	$replacements = @{
		'{{LINKS}}'    = ($links -join "`n")
		'{{REPO_URL}}' = $sourceUrl
		'{{ICON}}'     = "data:image/svg+xml;base64,$favicon"
		'/*{{CSS}}*/'  = ($css.Split("`n") | Join-String -Separator "`n$sep")
	}

	$html = switch ($html) {
		default { $h = $html; foreach ($key in $replacements.Keys) { $h = $h -replace [regex]::Escape($key), $replacements[$key] }; $h }
	}

	$html | Set-Content -LiteralPath (Join-Path $docDir 'index.html') -Encoding utf8
}
