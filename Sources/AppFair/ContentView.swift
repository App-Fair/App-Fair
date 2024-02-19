// This is free software: you can redistribute and/or modify it
// under the terms of the GNU Lesser General Public License 3.0
// as published by the Free Software Foundation https://fsf.org

import SwiftUI

@available(iOS 17.2, *)
public struct ContentView: View {
    @AppStorage("setting") var setting = true

    public init() {
    }

    public var body: some View {
        TabView {
            AppList()
                .tabItem { Label("Apps", systemImage: "list.bullet") }

            VStack {
                Text("Welcome to the App Fair!")
                Image(systemName: "heart.fill")
                    .foregroundColor(.red)
            }
            .font(.largeTitle)
            .tabItem { Label("App Fair", systemImage: "house.fill") }

            Form {
                Text("Settings")
                    .font(.largeTitle)
                Toggle("Option", isOn: $setting)
            }
            .tabItem { Label("Settings", systemImage: "gearshape.fill") }
        }
    }
}

@available(iOS 17.2, *)
public struct AppList: View {
    @State var viewModel = ViewModel()

    public init() {
    }

    public var body: some View {
        NavigationStack {
        }
        .task {
        }
    }

    @ViewBuilder func appDetailView(id: String) -> some View {
        VStack {
            Text("APP DETAIL")
        }
    }
}


/// Defines a model that obtains a list of managed apps.
@Observable public final class ViewModel {
//    @Published var content: [ManagedApp] = []
//    @Published var error: Error? = nil
//
//    @MainActor func getApps() async {
//        do {
//            for try await result in ManagedAppLibrary.currentDistributor.availableApps {
//                self.content = try result.get()
//            }
//        } catch {
//            self.error = error
//        }
//    }
}

