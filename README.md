<a name="readme-top"></a>

<br />
<div align="center">
  <img src="assets/ridge-logo-512.png" alt="Ridge" width="96" height="96">
  <h3 align="center">Ridge</h3>
</div>

## Table of Contents
- [About The Project](#about-the-project)
  - [Built With](#built-with)
- [Getting Started](#getting-started)
  - [Download](#download)
  - [Installation](#installation)
- [Usage](#usage)
- [Repository Layout](#repository-layout)
- [License](#license)

## About The Project
**Ridge** is a Windows desktop app for OSINT investigations, with five tabs:

- **OSINT tools**: the 1,700+ tools of the [Awesome-OSINT-List](https://github.com/Astrosp/Awesome-OSINT-List), searchable by name, category and type. Ridge checks which sites are up, and one click opens a tool in your browser.
- **Personas**: identity sheets and isolated browsers for sock-puppet accounts, each running in its own Docker container. Each persona keeps its own history, bookmarks, logins and notes between sessions.
- **Graph**: investigation graphs of people, accounts, places, money, infrastructure and more, linked together and saved to files.
- **Annotate**: pictures marked with numbered, coloured boxes and captions, exported as one PNG.
- **Settings**: what the personas' browsers are kept away from, and checks that prove it.

### Built With
* [![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
* [![egui](https://img.shields.io/badge/egui-0.37-000000)](https://github.com/emilk/egui)
* [![Docker](https://img.shields.io/badge/Docker-2496ED?logo=docker&logoColor=white)](https://www.docker.com/)
* [![Inno Setup](https://img.shields.io/badge/Inno_Setup-6-264de4)](https://jrsoftware.org/isinfo.php)

## Getting Started

### Download
Get the latest [release](https://github.com/Ridge-client/Launcher/releases):

| File | Platform |
| --- | --- |
| `Ridge-Setup-X.Y.Z.exe` | Windows 10/11. Installs without administrator rights and adds Ridge to the Start menu |

The Personas tab needs [Docker Desktop](https://www.docker.com/products/docker-desktop/) running; the other tabs work without it.

> [!NOTE]
> Releases are not code-signed yet, so Windows Smart App Control blocks Ridge. Builds will be signed with Azure Artifact Signing once its secrets are added to the repository.

### Installation
To build it yourself on Windows instead:
1. Install [Rust](https://rustup.rs/) and [Docker Desktop](https://www.docker.com/products/docker-desktop/)
2. Clone the repo
   ```sh
   git clone git@github.com:Ridge-client/Launcher.git
   ```
3. Build and run
   ```sh
   cd Launcher
   cargo run --release
   ```

## Usage
**OSINT tools**: type in the search box or pick a category. Each time Ridge opens it checks every site and marks it `ACTIVE` (up), `GUARDED` (up, but blocks automated checks) or `DOWN`; **Hide down sites** leaves those out. The **Type** column tells a website from a tool to download (GitHub, PyPI…), and **Open** opens it in your browser.

**Personas**: press **Create persona** to fill in an identity sheet (names, birth, documents, address, job, vehicle, profile picture…). The form sits where the Profile panel does; the ID generator sites from the OSINT list open in a built-in browser on its left, so values can be copied across. **Save persona** adds it to your list, and Ridge downloads the persona browser in the background (once, about 1 GB). Press **Start** in the list when ready: a short checklist asks you to confirm your VPN is on and the timezone matches its country, since the persona's browser goes online through your own connection, and checks the Settings protections from a test persona; **Start persona** waits until they pass. From the list you can also **Open browser**, **Stop**, view the **Profile**, **Edit** it, **Remove containers** (the browser profile is kept) or **Delete** it. Each persona's browser starts with its old bookmarks cleared and two folders on its toolbar: **Integrity**, to test that nothing leaks (IP and ISP, DNS, WebRTC, JavaScript, location, fingerprint, graphics card, camera, microphone, USB, network, plus links to this PC and your router that should not load), and **Social media**.

**Graph**: an investigation board. Ridge opens on a blank graph: **Save** (Ctrl+S; right-click or Ctrl+Shift+S to save under another name) writes it to a `.rgraph` file, **Open…** brings one back, and Ridge asks before unsaved changes would be lost. Name the graph above the board. Three tools set what dragging does: **Select** (V) moves entities and draws a selection box on empty board, **Pan** (H) moves around, **Link** (L) links one entity to another (the middle mouse button always pans). Add entities from the palette on the left — about a hundred kinds in Maltego's categories (cryptocurrency, devices, infrastructure, malware, MISP, PassiveTotal, Peering DB, penetration testing, people, personal, social network, tracking, transportation, weapons, Wikimedia…), with a search box and the recently used ones on top — by clicking or dragging them onto the board; entities take their category's colour. Drag an entity's ● onto another to link them. Block, hierarchical, circular and organic layouts arrange the selection or the whole graph, and the interactive layout keeps settling while you move entities. The right panel sets an entity's border (solid, dotted, short or long dashes) and a link's line style and arrow (none, either way or both), and fills in the selected entity's facts (a building's type, address and country, a crypto wallet's network…), a picture of it (kept inside the graph file), its notes and its links' labels.

**Annotate**: an intelligence image. **Open picture…** (or drop a picture on the window), then drag on it to draw an open box: each gets the next number and the chosen colour. Drag a box's edge to move it and its corner to resize it, write a caption for each on the right, and **Export PNG…** saves the picture with its numbered boxes and a legend of the captions. One picture at a time; nothing is kept between sessions.

**Settings**: what every persona's browser is kept away from: **this PC**, **your home network** and **the other personas** (a small firewall container holds each persona's network, and the browser cannot change its rules), plus a **password on each remote screen** that Ridge fills in for you. All are on by default, and the boxes show what the running personas really use. **Apply** saves them and restarts the running personas that don't use them yet. Below them, **Run checks** makes a temporary test persona with the applied protections (no need to start one) and, along with each running persona, tries to reach your PC, router, the other personas, Docker and your hardware, and shows what needs attention in red.

## Repository Layout
```
src/
  gui/          The desktop window (OSINT tools, Personas, Graph, Annotate and Settings tabs)
  catalog/      Tool catalog and website status checks
  persona/      Docker containers, VPN and browser settings for personas
  graph.rs      Investigation graphs: entity types, links, layouts
  annotate.rs   Annotated images: numbered boxes and the PNG export
data/           The OSINT tool catalog
installer/      Windows Setup (Inno Setup)
tools/          Build, signing, release-notes and catalog import scripts
```

## License
Ridge is free software under the [GNU General Public License v3.0](LICENSE). The icons are [Phosphor Icons](https://phosphoricons.com) ([MIT](assets/Phosphor-LICENSE.txt)), and the catalog comes from the [Awesome-OSINT-List](https://github.com/Astrosp/Awesome-OSINT-List).

<p align="right">(<a href="#readme-top">back to top</a>)</p>
